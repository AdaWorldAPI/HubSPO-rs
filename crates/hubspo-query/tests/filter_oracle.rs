//! D-QRY-1 against a plain-loop oracle: random HubSpot filter groups are
//! compiled, lowered and run on mask-risc, and the kept rows must equal what
//! a row-by-row reading of the same filter keeps.

use hubspo_query::filter::{CompileError, FilterGroup, Operator, PropertyFilter, compile};
use hubspo_store::catalog::{Catalog, Codebook, FieldSpec};
use lance_graph_mask_risc::{Foreign, LaneRef, Out, Planes, Scratch, execute_into, words_for};
use lance_graph_quack::bind::{FieldKind, create_table};
use lance_graph_quack::{Agg, Col, Mask, Query, lower};

const STAGES: &[&str] = &[
    "subscriber",
    "lead",
    "marketingqualifiedlead",
    "salesqualifiedlead",
    "opportunity",
    "customer",
];
const COUNTRIES: &[&str] = &["DE", "AT", "CH", "FR", "NL", "US", "GB", "PL"];
const N: usize = 5_000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// The records as the oracle reads them; `None` is "no value".
struct Records {
    live: Vec<bool>,
    stage: Vec<u32>,
    country: Vec<Option<u32>>,
    amount: Vec<Option<i32>>,
    created: Vec<i32>,
}

/// The same records as resident lanes and planes. A missing value keeps a
/// random payload in its lane, so a compiler that reads it gets caught.
struct Resident {
    stage: Vec<u32>,
    country: Vec<u32>,
    amount: Vec<i32>,
    created: Vec<i32>,
    masks: [Vec<u64>; 3],
}

fn bits(n: usize, f: impl Fn(usize) -> bool) -> Vec<u64> {
    let mut w = vec![0u64; words_for(n)];
    for i in 0..n {
        if f(i) {
            w[i / 64] |= 1 << (i % 64);
        }
    }
    w
}

fn records() -> (Records, Resident) {
    let mut r = Rng(0xD_0001);
    let mut rec = Records {
        live: Vec::new(),
        stage: Vec::new(),
        country: Vec::new(),
        amount: Vec::new(),
        created: Vec::new(),
    };
    let mut res = Resident {
        stage: Vec::new(),
        country: Vec::new(),
        amount: Vec::new(),
        created: Vec::new(),
        masks: [Vec::new(), Vec::new(), Vec::new()],
    };
    for _ in 0..N {
        rec.live.push(!r.chance(3));
        let stage = r.below(STAGES.len() as u64) as u32;
        rec.stage.push(stage);
        res.stage.push(stage);
        let country = r.below(COUNTRIES.len() as u64) as u32;
        let has_country = !r.chance(20);
        rec.country.push(has_country.then_some(country));
        res.country.push(country);
        let amount = r.below(5_500) as i32 - 500;
        let has_amount = !r.chance(30);
        rec.amount.push(has_amount.then_some(amount));
        res.amount.push(amount);
        let created = r.below(365) as i32;
        rec.created.push(created);
        res.created.push(created);
    }
    res.masks = [
        bits(N, |i| rec.live[i]),
        bits(N, |i| rec.country[i].is_some()),
        bits(N, |i| rec.amount[i].is_some()),
    ];
    (rec, res)
}

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    let t = create_table("contacts")
        .width(16)
        .register(&mut c)
        .expect("register")
        .id;
    let field = |name: &str, col: u16, kind, validity, labels: Option<&[&str]>| FieldSpec {
        name: name.into(),
        col: Col(col),
        kind,
        validity,
        codebook: labels.map(|l| Codebook::new(l.iter().copied()).expect("codebook")),
    };
    for spec in [
        field("lifecyclestage", 0, FieldKind::Code, None, Some(STAGES)),
        field(
            "country",
            1,
            FieldKind::Code,
            Some(Mask(1)),
            Some(COUNTRIES),
        ),
        field("amount", 2, FieldKind::I32, Some(Mask(2)), None),
        field("createdate", 3, FieldKind::I32, None, None),
    ] {
        c.declare(t, spec).expect("declare");
    }
    c
}

// ---------------------------------------------------------------- oracle ----

/// One property of one record, as the oracle sees it.
enum Val {
    Code(Option<u32>),
    Int(Option<i32>),
}

fn value(rec: &Records, property: &str, row: usize) -> Val {
    match property {
        "lifecyclestage" => Val::Code(Some(rec.stage[row])),
        "country" => Val::Code(rec.country[row]),
        "amount" => Val::Int(rec.amount[row]),
        "createdate" => Val::Int(Some(rec.created[row])),
        other => panic!("no property {other}"),
    }
}

fn label_code(property: &str, label: &str) -> u32 {
    let list = if property == "lifecyclestage" {
        STAGES
    } else {
        COUNTRIES
    };
    list.iter().position(|l| *l == label).expect("label") as u32
}

/// HubSpot's reading of one filter on one record, with a missing value kept
/// by no comparison (the module's documented choice).
fn keeps(rec: &Records, f: &PropertyFilter, row: usize) -> bool {
    let v = value(rec, &f.property, row);
    let present = match &v {
        Val::Code(x) => x.is_some(),
        Val::Int(x) => x.is_some(),
    };
    match f.operator {
        Operator::HasProperty => return present,
        Operator::NotHasProperty => return !present,
        _ if !present => return false,
        _ => {}
    }
    let one = || f.value.as_deref().expect("value");
    match v {
        Val::Code(Some(x)) => {
            let set: Vec<u32> = f
                .values
                .iter()
                .map(|l| label_code(&f.property, l))
                .collect();
            match f.operator {
                Operator::Eq => x == label_code(&f.property, one()),
                Operator::Neq => x != label_code(&f.property, one()),
                Operator::In => set.contains(&x),
                Operator::NotIn => !set.contains(&x),
                op => panic!("{op:?} on a coded property"),
            }
        }
        Val::Int(Some(x)) => {
            let n = |s: &str| s.parse::<i32>().expect("int");
            let set: Vec<i32> = f.values.iter().map(|s| n(s)).collect();
            match f.operator {
                Operator::Eq => x == n(one()),
                Operator::Neq => x != n(one()),
                Operator::Lt => x < n(one()),
                Operator::Lte => x <= n(one()),
                Operator::Gt => x > n(one()),
                Operator::Gte => x >= n(one()),
                Operator::Between => {
                    x >= n(one()) && x <= n(f.high_value.as_deref().expect("high"))
                }
                Operator::In => set.contains(&x),
                Operator::NotIn => !set.contains(&x),
                op => panic!("{op:?} on an integer property"),
            }
        }
        _ => unreachable!("missing values returned above"),
    }
}

fn oracle(rec: &Records, groups: &[FilterGroup]) -> Vec<usize> {
    (0..N)
        .filter(|&row| {
            rec.live[row]
                && (groups.is_empty()
                    || groups
                        .iter()
                        .any(|g| g.filters.iter().all(|f| keeps(rec, f, row))))
        })
        .collect()
}

// ----------------------------------------------------------------- engine ----

fn engine(res: &Resident, cat: &Catalog, groups: &[FilterGroup]) -> Vec<usize> {
    let filter = compile("contacts", groups, cat).expect("compile");
    let program = lower(&Query {
        filter,
        agg: Agg::Rows,
    })
    .expect("lower");
    let masks: Vec<&[u64]> = res.masks.iter().map(Vec::as_slice).collect();
    let lanes = [
        LaneRef::U32(&res.stage),
        LaneRef::U32(&res.country),
        LaneRef::I32(&res.amount),
        LaneRef::I32(&res.created),
    ];
    let planes = Planes {
        n_rows: N,
        masks: &masks,
        lanes: &lanes,
    };
    let mut out = vec![0u64; words_for(N)];
    let mut scratch = Scratch::for_program(&program, N).expect("scratch");
    execute_into(
        &program,
        &planes,
        &Foreign::NONE,
        &mut scratch,
        Out::Mask(&mut out),
    )
    .expect("execute");
    lance_graph_mask_risc::materialize_rows(&out, N)
}

// ------------------------------------------------------------- generator ----

fn random_filter(r: &mut Rng) -> PropertyFilter {
    let pick = |r: &mut Rng, l: &[&'static str]| l[r.below(l.len() as u64) as usize];
    let int = |r: &mut Rng, lo: i64, hi: i64| (lo + r.below((hi - lo) as u64) as i64).to_string();
    match r.below(4) {
        p @ (0 | 1) => {
            let (name, labels) = if p == 0 {
                ("lifecyclestage", STAGES)
            } else {
                ("country", COUNTRIES)
            };
            match r.below(6) {
                0 => PropertyFilter::new(name, Operator::Eq, pick(r, labels)),
                1 => PropertyFilter::new(name, Operator::Neq, pick(r, labels)),
                2 | 3 => {
                    let k = 1 + r.below(3) as usize;
                    let vs: Vec<&str> = (0..k).map(|_| pick(r, labels)).collect();
                    let op = if r.chance(50) {
                        Operator::In
                    } else {
                        Operator::NotIn
                    };
                    PropertyFilter::set(name, op, &vs)
                }
                4 => PropertyFilter::presence(name, Operator::HasProperty),
                _ => PropertyFilter::presence(name, Operator::NotHasProperty),
            }
        }
        p => {
            let (name, lo, hi) = if p == 2 {
                ("amount", -600, 5_100)
            } else {
                ("createdate", -5, 370)
            };
            match r.below(10) {
                0 => PropertyFilter::new(name, Operator::Eq, &int(r, lo, hi)),
                1 => PropertyFilter::new(name, Operator::Neq, &int(r, lo, hi)),
                2 => PropertyFilter::new(name, Operator::Lt, &int(r, lo, hi)),
                3 => PropertyFilter::new(name, Operator::Lte, &int(r, lo, hi)),
                4 => PropertyFilter::new(name, Operator::Gt, &int(r, lo, hi)),
                5 => PropertyFilter::new(name, Operator::Gte, &int(r, lo, hi)),
                6 => {
                    let a = int(r, lo, hi);
                    let b = int(r, lo, hi);
                    PropertyFilter::between(name, &a, &b)
                }
                7 => {
                    let vs: Vec<String> = (0..3).map(|_| int(r, lo, hi)).collect();
                    let vs: Vec<&str> = vs.iter().map(String::as_str).collect();
                    let op = if r.chance(50) {
                        Operator::In
                    } else {
                        Operator::NotIn
                    };
                    PropertyFilter::set(name, op, &vs)
                }
                8 => PropertyFilter::presence(name, Operator::HasProperty),
                _ => PropertyFilter::presence(name, Operator::NotHasProperty),
            }
        }
    }
}

fn random_groups(r: &mut Rng) -> Vec<FilterGroup> {
    (0..1 + r.below(3))
        .map(|_| FilterGroup {
            filters: (0..1 + r.below(4)).map(|_| random_filter(r)).collect(),
        })
        .collect()
}

// ------------------------------------------------------------------ tests ----

#[test]
fn random_filter_groups_keep_exactly_the_oracle_rows() {
    let (rec, res) = records();
    let cat = catalog();
    let mut r = Rng(0xF1_17E5);
    let (mut empty, mut full, mut nontrivial) = (0, 0, 0);
    let live = oracle(&rec, &[]).len();
    for case in 0..600 {
        let groups = random_groups(&mut r);
        let want = oracle(&rec, &groups);
        let got = engine(&res, &cat, &groups);
        assert_eq!(got, want, "case {case}: {groups:?}");
        match want.len() {
            0 => empty += 1,
            n if n == live => full += 1,
            _ => nontrivial += 1,
        }
    }
    // Anti-vacuity: most cases must select a proper, non-empty subset, or
    // agreement would say little.
    eprintln!("cases: {nontrivial} non-trivial, {empty} empty, {full} all live");
    assert!(
        nontrivial > 450,
        "only {nontrivial} non-trivial cases ({empty} empty, {full} full)"
    );
}

#[test]
fn no_groups_is_every_live_record() {
    let (rec, res) = records();
    let got = engine(&res, &catalog(), &[]);
    assert_eq!(got.len(), rec.live.iter().filter(|l| **l).count());
    assert!(got.len() < N, "the fixture must delete some records");
}

#[test]
fn a_missing_value_is_kept_by_no_comparison() {
    let (rec, res) = records();
    let cat = catalog();
    for f in [
        PropertyFilter::new("country", Operator::Neq, "DE"),
        PropertyFilter::set("country", Operator::NotIn, &["DE", "AT"]),
        PropertyFilter::new("amount", Operator::Neq, "0"),
        PropertyFilter::set("amount", Operator::NotIn, &["1", "2"]),
    ] {
        let got = engine(
            &res,
            &cat,
            &[FilterGroup {
                filters: vec![f.clone()],
            }],
        );
        assert!(!got.is_empty(), "{f:?} keeps nothing");
        let missing = |row: usize| match f.property.as_str() {
            "country" => rec.country[row].is_none(),
            _ => rec.amount[row].is_none(),
        };
        assert!(
            got.iter().all(|&row| !missing(row)),
            "{f:?} kept a record with no value"
        );
    }
}

#[test]
fn presence_on_a_required_property_is_all_or_nothing() {
    let (rec, res) = records();
    let cat = catalog();
    let live = rec.live.iter().filter(|l| **l).count();
    let has = engine(
        &res,
        &cat,
        &[FilterGroup {
            filters: vec![PropertyFilter::presence(
                "createdate",
                Operator::HasProperty,
            )],
        }],
    );
    let lacks = engine(
        &res,
        &cat,
        &[FilterGroup {
            filters: vec![PropertyFilter::presence(
                "createdate",
                Operator::NotHasProperty,
            )],
        }],
    );
    assert_eq!((has.len(), lacks.len()), (live, 0));
}

#[test]
fn malformed_filters_are_refused_by_name() {
    let cat = catalog();
    let one = |f: PropertyFilter| compile("contacts", &[FilterGroup { filters: vec![f] }], &cat);
    assert_eq!(
        compile("deals", &[], &cat),
        Err(CompileError::UnknownObject("deals".into()))
    );
    assert_eq!(
        one(PropertyFilter::new("email", Operator::Eq, "a@b.c")),
        Err(CompileError::UnknownProperty("email".into()))
    );
    assert_eq!(
        one(PropertyFilter::new(
            "lifecyclestage",
            Operator::Eq,
            "evangelist"
        )),
        Err(CompileError::UnknownValue {
            property: "lifecyclestage".into(),
            value: "evangelist".into()
        })
    );
    assert_eq!(
        one(PropertyFilter::set("country", Operator::In, &["DE", "XX"])),
        Err(CompileError::UnknownValue {
            property: "country".into(),
            value: "XX".into()
        })
    );
    assert_eq!(
        one(PropertyFilter::new("amount", Operator::Gt, "100.50")),
        Err(CompileError::NotAnInteger {
            property: "amount".into(),
            value: "100.50".into()
        })
    );
    assert_eq!(
        one(PropertyFilter::new("lifecyclestage", Operator::Lt, "lead")),
        Err(CompileError::OperatorNotSupported {
            property: "lifecyclestage".into(),
            operator: Operator::Lt
        })
    );
    assert_eq!(
        one(PropertyFilter::new("country", Operator::ContainsToken, "D")),
        Err(CompileError::TextSearch {
            property: "country".into()
        })
    );
    assert_eq!(
        one(PropertyFilter::set("amount", Operator::In, &[])),
        Err(CompileError::MissingValue {
            property: "amount".into(),
            operator: Operator::In
        })
    );
    assert_eq!(
        one(PropertyFilter {
            high_value: None,
            ..PropertyFilter::between("amount", "1", "2")
        }),
        Err(CompileError::MissingValue {
            property: "amount".into(),
            operator: Operator::Between
        })
    );
    assert_eq!(
        compile("contacts", &[FilterGroup::default()], &cat),
        Err(CompileError::EmptyGroup)
    );
}

#[test]
fn hubspot_operator_names_round_trip() {
    for name in [
        "EQ",
        "NEQ",
        "LT",
        "LTE",
        "GT",
        "GTE",
        "BETWEEN",
        "IN",
        "NOT_IN",
        "HAS_PROPERTY",
        "NOT_HAS_PROPERTY",
        "CONTAINS_TOKEN",
        "NOT_CONTAINS_TOKEN",
    ] {
        assert!(Operator::from_hubspot(name).is_some(), "{name}");
    }
    assert_eq!(Operator::from_hubspot("eq"), None);
}
