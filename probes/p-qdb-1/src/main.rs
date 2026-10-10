//! P-QDB-1 — Quack as the database for tickets, mail, documents and delivery.
//!
//! Question: can HubSPO-rs keep its comms tables (tickets, messages,
//! deliveries, documents) as Lance datasets and answer every read through
//! Quack — names and text bound once by a HubSPO-owned catalog, then
//! numeric programs executed by mask-risc over the loaded columns — with
//! answers equal to a plain-loop oracle and latencies inside the 50 ms
//! interactive budget?
//!
//! What this probe owns, because nothing upstream does yet:
//! - the `Registrar` (table catalog): `lance-graph-quack::bind` names the seam
//!   and says only tests implement it;
//! - the `Binder` (field catalog + per-field codebook): text literals become
//!   `u32` codes once, and no string ever reaches a lane;
//! - residency: each Lance column is loaded once into one contiguous Arrow
//!   buffer, and every query borrows it (`LaneRef::U32(array.values())`).
//!
//! Tables (all columns `u32` codes / foreign keys or `i32`):
//! - `user`      team, active                       (the dir-sim side, synthetic)
//! - `ticket`    status, pipeline, owner→user, priority, created_day
//! - `message`   mailbox (address codebook → user), ticket→ticket, seen, day
//! - `delivery`  message→message, outcome, day
//! - `document`  ticket→ticket, folder, size_kb
//!
//! Usage: `p-qdb-1 <tickets> [disable] [clustered]`, disable ∈ {none, code,
//! via, live}; `clustered` stores deliveries grouped by message.
//! A disable perturbs one step and must make an oracle assert fail.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Array, ArrayRef, Int32Array, RecordBatch, RecordBatchIterator, UInt32Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;
use lance_graph_mask_risc::{
    Foreign, LaneRef, Out, Planes, Program, Scratch, Value, execute_into, materialize_rows,
    words_for,
};
use lance_graph_quack::bind::{
    BindError, Binder, BoundField, FieldKind, RegisterError, Registrar, TableId, create_table,
    table,
};
use lance_graph_quack::{
    Agg, Cmp, Col, Filter, ForeignLane, GroupAddr, GroupAgg, Mask, Query, lower,
    normalize_group_sink,
};

// ---------------------------------------------------------------- data ----

enum ColData {
    U32(Vec<u32>),
    I32(Vec<i32>),
}

struct TableData {
    name: &'static str,
    cols: Vec<(&'static str, ColData)>,
}

impl TableData {
    fn rows(&self) -> usize {
        match &self.cols[0].1 {
            ColData::U32(v) => v.len(),
            ColData::I32(v) => v.len(),
        }
    }
    fn u32(&self, name: &str) -> &[u32] {
        match self.cols.iter().find(|c| c.0 == name).map(|c| &c.1) {
            Some(ColData::U32(v)) => v,
            _ => panic!("{name} is not a u32 column of {}", self.name),
        }
    }
}

/// Deterministic generator (SplitMix64).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n)) as u32
    }
    /// Index into `weights` (percent, summing to 100).
    fn weighted(&mut self, weights: &[u32]) -> u32 {
        let mut r = self.below(100);
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                return i as u32;
            }
            r -= w;
        }
        weights.len() as u32 - 1
    }
}

const STATUS: &[&str] = &["open", "pending", "waiting", "closed"];
const PIPELINE: &[&str] = &["support", "billing", "onboarding"];
const PRIORITY: &[&str] = &["low", "medium", "high"];
const SEEN: &[&str] = &["unseen", "seen"];
const OUTCOME: &[&str] = &["delivered", "deferred", "bounced", "complained"];
const FOLDER: &[&str] = &["contracts", "invoices", "screenshots", "other"];
const ACTIVE: &[&str] = &["active", "disabled"];
const TEAMS: u32 = 50;

fn generate(tickets: usize, users: usize, clustered: bool) -> Vec<TableData> {
    let mut r = Rng(0x5EED);
    let u = users as u32;
    let user = TableData {
        name: "user",
        cols: vec![
            (
                "team",
                ColData::U32((0..u).map(|_| r.below(TEAMS)).collect()),
            ),
            (
                "active",
                ColData::U32((0..u).map(|_| r.weighted(&[95, 5])).collect()),
            ),
        ],
    };
    let t = tickets as u32;
    let ticket = TableData {
        name: "ticket",
        cols: vec![
            (
                "status",
                ColData::U32((0..t).map(|_| r.weighted(&[40, 15, 10, 35])).collect()),
            ),
            (
                "pipeline",
                ColData::U32((0..t).map(|_| r.weighted(&[60, 25, 15])).collect()),
            ),
            ("owner", ColData::U32((0..t).map(|_| r.below(u)).collect())),
            (
                "priority",
                ColData::U32((0..t).map(|_| r.weighted(&[50, 35, 15])).collect()),
            ),
            (
                "created_day",
                ColData::I32((0..t).map(|_| r.below(365) as i32).collect()),
            ),
        ],
    };
    let m = 4 * t;
    let message = TableData {
        name: "message",
        cols: vec![
            (
                "mailbox",
                ColData::U32((0..m).map(|_| r.below(u)).collect()),
            ),
            ("ticket", ColData::U32((0..m).map(|_| r.below(t)).collect())),
            (
                "seen",
                ColData::U32((0..m).map(|_| r.weighted(&[30, 70])).collect()),
            ),
            (
                "day",
                ColData::I32((0..m).map(|_| r.below(365) as i32).collect()),
            ),
        ],
    };
    let d = 4 * t;
    let delivery = TableData {
        name: "delivery",
        cols: vec![
            (
                "message",
                ColData::U32({
                    let mut v: Vec<u32> = (0..d).map(|_| r.below(m)).collect();
                    // Deliveries are written as messages are sent, so in storage
                    // they arrive grouped by message. `clustered` models that;
                    // the default (random) is the worst case for an fk gather.
                    if clustered {
                        v.sort_unstable();
                    }
                    v
                }),
            ),
            (
                "outcome",
                ColData::U32((0..d).map(|_| r.weighted(&[90, 4, 5, 1])).collect()),
            ),
            (
                "day",
                ColData::I32((0..d).map(|_| r.below(365) as i32).collect()),
            ),
        ],
    };
    let document = TableData {
        name: "document",
        cols: vec![
            ("ticket", ColData::U32((0..t).map(|_| r.below(t)).collect())),
            (
                "folder",
                ColData::U32((0..t).map(|_| r.weighted(&[20, 30, 30, 20])).collect()),
            ),
            (
                "size_kb",
                ColData::I32((0..t).map(|_| 1 + r.below(10_000) as i32).collect()),
            ),
        ],
    };
    vec![user, ticket, message, delivery, document]
}

// ------------------------------------------------------------- storage ----

fn schema_of(t: &TableData) -> Arc<Schema> {
    Arc::new(Schema::new(
        t.cols
            .iter()
            .map(|(n, c)| {
                let ty = match c {
                    ColData::U32(_) => DataType::UInt32,
                    ColData::I32(_) => DataType::Int32,
                };
                Field::new(*n, ty, false)
            })
            .collect::<Vec<_>>(),
    ))
}

async fn write_table(uri: &str, t: &TableData) {
    let schema = schema_of(t);
    let n = t.rows();
    let chunk = 1 << 20;
    let mut batches = Vec::new();
    let mut s = 0;
    while s < n {
        let e = (s + chunk).min(n);
        let cols: Vec<ArrayRef> = t
            .cols
            .iter()
            .map(|(_, c)| match c {
                ColData::U32(v) => Arc::new(UInt32Array::from(v[s..e].to_vec())) as ArrayRef,
                ColData::I32(v) => Arc::new(Int32Array::from(v[s..e].to_vec())) as ArrayRef,
            })
            .collect();
        batches.push(Ok(
            RecordBatch::try_new(schema.clone(), cols).expect("batch")
        ));
        s = e;
    }
    Dataset::write(RecordBatchIterator::new(batches, schema), uri, None)
        .await
        .expect("write table");
}

/// One table's columns resident in memory: every column one contiguous
/// Arrow buffer (the only copy made, at load), plus the live plane.
struct Resident {
    n: usize,
    live: Vec<u64>,
    cols: Vec<ArrayRef>,
}

impl Resident {
    fn lanes(&self) -> Vec<LaneRef<'_>> {
        self.cols
            .iter()
            .map(|a| {
                if let Some(u) = a.as_any().downcast_ref::<UInt32Array>() {
                    LaneRef::U32(u.values())
                } else if let Some(i) = a.as_any().downcast_ref::<Int32Array>() {
                    LaneRef::I32(i.values())
                } else {
                    panic!("unsupported lane type {:?}", a.data_type())
                }
            })
            .collect()
    }
    fn lane(&self, col: usize) -> LaneRef<'_> {
        self.lanes()[col]
    }
}

async fn load_table(uri: &str) -> Resident {
    let ds = Dataset::open(uri).await.expect("open");
    let batches: Vec<RecordBatch> = ds
        .scan()
        .try_into_stream()
        .await
        .expect("scan")
        .try_collect()
        .await
        .expect("collect");
    let schema = batches[0].schema();
    let cols: Vec<ArrayRef> = (0..schema.fields().len())
        .map(|c| {
            let parts: Vec<&dyn Array> = batches.iter().map(|b| b.column(c).as_ref()).collect();
            arrow::compute::concat(&parts).expect("concat")
        })
        .collect();
    let n = cols[0].len();
    let mut live = vec![u64::MAX; words_for(n)];
    if n % 64 != 0 {
        *live.last_mut().expect("words") = (1u64 << (n % 64)) - 1;
    }
    Resident { n, live, cols }
}

// ------------------------------------------------------------- catalog ----

/// HubSPO-rs's catalog: the production `Registrar` + `Binder` that
/// `lance-graph-quack::bind` names and nobody implements yet.
#[derive(Default)]
struct Catalog {
    tables: Vec<TableMeta>,
}

struct TableMeta {
    name: String,
    fields: Vec<(String, BoundField)>,
    /// Per field column: literal → code. Never minted at bind time.
    codes: HashMap<u16, HashMap<String, u32>>,
}

impl Registrar for Catalog {
    fn register(&mut self, name: &str, width: u32) -> Result<TableId, RegisterError> {
        if self.tables.iter().any(|t| t.name == name) {
            return Err(RegisterError::Exists(name.into()));
        }
        if width % 4 != 0 {
            return Err(RegisterError::Refused {
                table: name.into(),
                width,
            });
        }
        self.tables.push(TableMeta {
            name: name.into(),
            fields: Vec::new(),
            codes: HashMap::new(),
        });
        Ok(TableId(self.tables.len() as u32 - 1))
    }
}

impl Catalog {
    fn declare(&mut self, t: TableId, field: &str, col: u16, kind: FieldKind, codebook: &[String]) {
        let meta = &mut self.tables[t.0 as usize];
        meta.fields.push((
            field.into(),
            BoundField {
                col: Col(col),
                kind,
                validity: None,
            },
        ));
        if !codebook.is_empty() {
            meta.codes.insert(
                col,
                codebook
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (s.clone(), i as u32))
                    .collect(),
            );
        }
    }
}

impl Binder for Catalog {
    fn table(&self, name: &str) -> Option<TableId> {
        self.tables
            .iter()
            .position(|t| t.name == name)
            .map(|i| TableId(i as u32))
    }
    /// Each table's resident `masks[0]` is its live plane.
    fn live(&self, _table: TableId) -> Mask {
        Mask(0)
    }
    fn field(&self, table: TableId, name: &str) -> Option<BoundField> {
        let meta = self.tables.get(table.0 as usize)?;
        meta.fields.iter().find(|f| f.0 == name).map(|f| f.1)
    }
    fn code(&self, table: TableId, field: Col, literal: &str) -> Option<u32> {
        self.tables
            .get(table.0 as usize)?
            .codes
            .get(&field.0)?
            .get(literal)
            .copied()
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn addresses(users: usize) -> Vec<String> {
    (0..users)
        .map(|i| format!("user{i}@example.test"))
        .collect()
}

fn build_catalog(users: usize) -> Catalog {
    use FieldKind::{Code, I32};
    let mut c = Catalog::default();
    let none: Vec<String> = Vec::new();
    let reg = |c: &mut Catalog, name: &str, cols: u32| {
        create_table(name)
            .width(4 * cols)
            .register(c)
            .expect("register")
            .id
    };
    let t = reg(&mut c, "user", 2);
    c.declare(
        t,
        "team",
        0,
        Code,
        &(0..TEAMS).map(|i| format!("team-{i}")).collect::<Vec<_>>(),
    );
    c.declare(t, "active", 1, Code, &names(ACTIVE));
    let t = reg(&mut c, "ticket", 5);
    c.declare(t, "status", 0, Code, &names(STATUS));
    c.declare(t, "pipeline", 1, Code, &names(PIPELINE));
    c.declare(t, "owner", 2, Code, &addresses(users));
    c.declare(t, "priority", 3, Code, &names(PRIORITY));
    c.declare(t, "created_day", 4, I32, &none);
    let t = reg(&mut c, "message", 4);
    c.declare(t, "mailbox", 0, Code, &addresses(users));
    c.declare(t, "ticket", 1, Code, &none);
    c.declare(t, "seen", 2, Code, &names(SEEN));
    c.declare(t, "day", 3, I32, &none);
    let t = reg(&mut c, "delivery", 3);
    c.declare(t, "message", 0, Code, &none);
    c.declare(t, "outcome", 1, Code, &names(OUTCOME));
    c.declare(t, "day", 2, I32, &none);
    let t = reg(&mut c, "document", 3);
    c.declare(t, "ticket", 0, Code, &none);
    c.declare(t, "folder", 1, Code, &names(FOLDER));
    c.declare(t, "size_kb", 2, I32, &none);
    c
}

// ----------------------------------------------------------- execution ----

/// Run `p` `reps` times; returns the last value and the median time in ms.
fn run(
    p: &Program,
    planes: &Planes<'_>,
    foreign: &Foreign<'_>,
    reps: usize,
    mut out: impl FnMut() -> OutBuf,
) -> (Value, OutBuf, f64) {
    let mut times = Vec::with_capacity(reps);
    let mut last = None;
    for _ in 0..reps {
        let mut buf = out();
        let mut scratch = Scratch::for_program(p, planes.n_rows).expect("scratch");
        let t0 = Instant::now();
        let v = match &mut buf {
            OutBuf::None => execute_into(p, planes, foreign, &mut scratch, Out::None),
            OutBuf::Mask(m) => execute_into(p, planes, foreign, &mut scratch, Out::Mask(m)),
            OutBuf::I64(s) => execute_into(p, planes, foreign, &mut scratch, Out::I64(s)),
        }
        .expect("execute");
        times.push(t0.elapsed().as_secs_f64() * 1e3);
        last = Some((v, buf));
    }
    times.sort_by(f64::total_cmp);
    let (v, buf) = last.expect("reps > 0");
    (v, buf, times[times.len() / 2])
}

enum OutBuf {
    None,
    Mask(Vec<u64>),
    I64(Vec<i64>),
}

fn count(v: &Value) -> usize {
    match v {
        Value::Count(n) => *n,
        other => panic!("expected a count, got {other:?}"),
    }
}

fn planes<'a>(r: &'a Resident, masks: &'a [&'a [u64]], lanes: &'a [LaneRef<'a>]) -> Planes<'a> {
    Planes {
        n_rows: r.n,
        masks,
        lanes,
    }
}

fn report(id: &str, what: &str, ms: f64, oracle_ms: f64, answer: impl std::fmt::Display) {
    println!("{id:<4} {what:<58} {ms:>9.3} ms  (loop {oracle_ms:>8.3} ms)  = {answer}");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let tickets: usize = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);
    let disable = args.get(2).cloned().unwrap_or_else(|| "none".into());
    let users = 20_000;
    let reps = 15;

    // -- generate, persist, reload ------------------------------------
    let t0 = Instant::now();
    let clustered = args.get(3).is_some_and(|a| a == "clustered");
    let data = generate(tickets, users, clustered);
    let gen_s = t0.elapsed().as_secs_f64();
    let dir = tempfile::tempdir().expect("tempdir");
    let t0 = Instant::now();
    for t in &data {
        write_table(&format!("{}/{}.lance", dir.path().display(), t.name), t).await;
    }
    let write_s = t0.elapsed().as_secs_f64();
    let t0 = Instant::now();
    let mut res = HashMap::new();
    for t in &data {
        res.insert(
            t.name,
            load_table(&format!("{}/{}.lance", dir.path().display(), t.name)).await,
        );
    }
    let load_s = t0.elapsed().as_secs_f64();
    let d: HashMap<&str, &TableData> = data.iter().map(|t| (t.name, t)).collect();
    for t in &data {
        assert_eq!(res[t.name].n, t.rows(), "{} reloaded row count", t.name);
    }
    let rows: Vec<String> = data
        .iter()
        .map(|t| format!("{} {}", t.name, t.rows()))
        .collect();
    println!("P-QDB-1  tickets={tickets} users={users} disable={disable} clustered={clustered}");
    println!("rows: {}", rows.join(", "));
    println!("generate {gen_s:.2} s · Lance write {write_s:.2} s · load to resident {load_s:.2} s");

    let mut cat = build_catalog(users);
    if disable == "code" {
        // Swap two status codes in the codebook: binding now names the wrong
        // population.
        let m = cat.tables[1].codes.get_mut(&0).expect("status codes");
        m.insert("open".into(), 1);
        m.insert("pending".into(), 0);
    }
    let code = |table: &str, field: &str, lit: &str| -> u32 {
        let t = cat.table(table).expect("table");
        let f = cat.field(t, field).expect("field");
        cat.code(t, f.col, lit).expect("literal")
    };

    let (ticket, message, delivery, document, user) = (
        &res["ticket"],
        &res["message"],
        &res["delivery"],
        &res["document"],
        &res["user"],
    );
    let live_mask = |r: &Resident| -> Vec<u64> {
        if disable == "live" {
            // Drop the last live word: the plane no longer covers every row.
            let mut l = r.live.clone();
            *l.last_mut().expect("words") = 0;
            l
        } else {
            r.live.clone()
        }
    };

    // -- tickets -------------------------------------------------------
    let t_live = live_mask(ticket);
    let t_masks: [&[u64]; 1] = [&t_live];
    let t_lanes = ticket.lanes();
    let tp = planes(ticket, &t_masks, &t_lanes);
    let td = d["ticket"];
    let (status, pipeline, owner) = (td.u32("status"), td.u32("pipeline"), td.u32("owner"));
    let team = d["user"].u32("team");

    let t_bind = Instant::now();
    let q1 = table("ticket")
        .where_eq("status", "open")
        .where_eq("pipeline", "support")
        .count();
    let q1 = q1.bind(&cat).expect("bind q1");
    let p1 = lower(&q1).expect("lower q1");
    let bind_ms = t_bind.elapsed().as_secs_f64() * 1e3;
    let lo = Instant::now();
    let o1 = (0..td.rows())
        .filter(|&i| status[i] == 0 && pipeline[i] == 0)
        .count();
    let o1_ms = lo.elapsed().as_secs_f64() * 1e3;
    let (v, _, ms) = run(&p1, &tp, &Foreign::NONE, reps, || OutBuf::None);
    report(
        "Q1",
        "tickets: open in pipeline 'support' (bound text)",
        ms,
        o1_ms,
        count(&v),
    );
    assert_eq!(count(&v), o1, "Q1 open support tickets");
    println!("     bind + lower once: {bind_ms:.3} ms");

    // Q2: routing — the owner's team, read through the fk into `user`.
    let team7 = 7u32;
    let via_lane = if disable == "via" {
        user.lane(1)
    } else {
        user.lane(0)
    };
    let user_lanes = [via_lane];
    let fgn = Foreign {
        planes: &[],
        lanes: &user_lanes,
    };
    let q2 = Query {
        filter: Filter::and([
            q1.filter.clone(),
            Filter::eq_u32_via(Col(2), ForeignLane(0), team7),
        ]),
        agg: Agg::Count,
    };
    let p2 = lower(&q2).expect("lower q2");
    let lo = Instant::now();
    let o2 = (0..td.rows())
        .filter(|&i| status[i] == 0 && pipeline[i] == 0 && team[owner[i] as usize] == team7)
        .count();
    let o2_ms = lo.elapsed().as_secs_f64() * 1e3;
    let (v, _, ms) = run(&p2, &tp, &fgn, reps, || OutBuf::None);
    report(
        "Q2",
        "tickets: … AND owner.team = team-7 (fk join, fused)",
        ms,
        o2_ms,
        count(&v),
    );
    assert_eq!(count(&v), o2, "Q2 team routing");

    // Q3: tickets GROUP BY status.
    let q3 = Query {
        filter: Filter::plane(Mask(0)),
        agg: Agg::GroupReduce {
            key: GroupAddr::Local(Col(0)),
            agg: GroupAgg::Count,
        },
    };
    let p3 = lower(&q3).expect("lower q3");
    let lo = Instant::now();
    let mut o3 = vec![0i64; STATUS.len()];
    for &s in status {
        o3[s as usize] += 1;
    }
    let o3_ms = lo.elapsed().as_secs_f64() * 1e3;
    let (v, buf, ms) = run(&p3, &tp, &Foreign::NONE, reps, || {
        OutBuf::I64(vec![0; STATUS.len()])
    });
    assert_eq!(v, Value::GroupReduced);
    let OutBuf::I64(g3) = buf else { unreachable!() };
    report(
        "Q3",
        "tickets: GROUP BY status COUNT",
        ms,
        o3_ms,
        format!("{g3:?}"),
    );
    assert_eq!(g3, o3, "Q3 status histogram");

    // -- mail ----------------------------------------------------------
    let m_live = live_mask(message);
    let m_masks: [&[u64]; 1] = [&m_live];
    let m_lanes = message.lanes();
    let mp = planes(message, &m_masks, &m_lanes);
    let md = d["message"];
    let (mailbox, m_ticket, seen) = (md.u32("mailbox"), md.u32("ticket"), md.u32("seen"));
    let who = "user42@example.test";
    let q4 = table("message")
        .where_eq("mailbox", who)
        .where_eq("seen", "unseen")
        .count();
    let p4 = lower(&q4.bind(&cat).expect("bind q4")).expect("lower q4");
    let lo = Instant::now();
    let o4 = (0..md.rows())
        .filter(|&i| mailbox[i] == 42 && seen[i] == 0)
        .count();
    let o4_ms = lo.elapsed().as_secs_f64() * 1e3;
    let (v, _, ms) = run(&p4, &mp, &Foreign::NONE, reps, || OutBuf::None);
    report(
        "Q4",
        "mail: unseen in mailbox user42@example.test",
        ms,
        o4_ms,
        count(&v),
    );
    assert_eq!(count(&v), o4, "Q4 unseen inbox");

    // Q5: the messages of one ticket's thread — rows, materialised by the
    // caller (the one named materialiser).
    let thread = 4242u32 % tickets as u32;
    let q5 = Query {
        filter: Filter::and([
            Filter::plane(Mask(0)),
            Filter::cmp(Col(1), Cmp::EqU32(thread)),
        ]),
        agg: Agg::Rows,
    };
    let p5 = lower(&q5).expect("lower q5");
    let lo = Instant::now();
    let o5: Vec<usize> = (0..md.rows()).filter(|&i| m_ticket[i] == thread).collect();
    let o5_ms = lo.elapsed().as_secs_f64() * 1e3;
    let words = words_for(message.n);
    let (_, buf, ms) = run(&p5, &mp, &Foreign::NONE, reps, || {
        OutBuf::Mask(vec![0; words])
    });
    let OutBuf::Mask(m5) = buf else {
        unreachable!()
    };
    let r5 = materialize_rows(&m5, message.n);
    report(
        "Q5",
        "mail: thread of one ticket (rows)",
        ms,
        o5_ms,
        format!("{} rows", r5.len()),
    );
    assert_eq!(r5, o5, "Q5 thread rows");

    // -- delivery ------------------------------------------------------
    let d_live = live_mask(delivery);
    let d_masks: [&[u64]; 1] = [&d_live];
    let d_lanes = delivery.lanes();
    let dp = planes(delivery, &d_masks, &d_lanes);
    let dd = d["delivery"];
    let (d_msg, outcome) = (dd.u32("message"), dd.u32("outcome"));

    let q6 = Query {
        filter: Filter::plane(Mask(0)),
        agg: Agg::GroupReduce {
            key: GroupAddr::Local(Col(1)),
            agg: GroupAgg::Count,
        },
    };
    let p6 = lower(&q6).expect("lower q6");
    let lo = Instant::now();
    let mut o6 = vec![0i64; OUTCOME.len()];
    for &o in outcome {
        o6[o as usize] += 1;
    }
    let o6_ms = lo.elapsed().as_secs_f64() * 1e3;
    let (_, buf, ms) = run(&p6, &dp, &Foreign::NONE, reps, || {
        OutBuf::I64(vec![0; OUTCOME.len()])
    });
    let OutBuf::I64(g6) = buf else { unreachable!() };
    report(
        "Q6",
        "delivery: GROUP BY outcome COUNT",
        ms,
        o6_ms,
        format!("{g6:?}"),
    );
    assert_eq!(g6, o6, "Q6 outcome histogram");

    // Q7: bounced deliveries of mail in one mailbox — fk into `message`.
    let bounced = code("delivery", "outcome", "bounced");
    let box42 = code("message", "mailbox", who);
    let msg_lanes = [message.lane(0)];
    let fgn_m = Foreign {
        planes: &[],
        lanes: &msg_lanes,
    };
    let q7 = Query {
        filter: Filter::and([
            Filter::plane(Mask(0)),
            Filter::cmp(Col(1), Cmp::EqU32(bounced)),
            Filter::eq_u32_via(Col(0), ForeignLane(0), box42),
        ]),
        agg: Agg::Count,
    };
    let p7 = lower(&q7).expect("lower q7");
    let lo = Instant::now();
    let o7 = (0..dd.rows())
        .filter(|&i| outcome[i] == 2 && mailbox[d_msg[i] as usize] == 42)
        .count();
    let o7_ms = lo.elapsed().as_secs_f64() * 1e3;
    let (v, _, ms) = run(&p7, &dp, &fgn_m, reps, || OutBuf::None);
    report(
        "Q7",
        "delivery: bounced AND message.mailbox = user42 (fk join)",
        ms,
        o7_ms,
        count(&v),
    );
    assert_eq!(count(&v), o7, "Q7 bounced for one mailbox");

    // Q8: deliveries per mailbox — GROUP BY through the fk, 20k groups.
    let q8 = Query {
        filter: Filter::plane(Mask(0)),
        agg: Agg::GroupReduce {
            key: GroupAddr::Via {
                fk: Col(0),
                key: ForeignLane(0),
            },
            agg: GroupAgg::Count,
        },
    };
    let p8 = lower(&q8).expect("lower q8");
    let lo = Instant::now();
    let mut o8 = vec![0i64; users];
    for &m in d_msg {
        o8[mailbox[m as usize] as usize] += 1;
    }
    let o8_ms = lo.elapsed().as_secs_f64() * 1e3;
    let (_, buf, ms) = run(&p8, &dp, &fgn_m, reps, || OutBuf::I64(vec![0; users]));
    let OutBuf::I64(mut g8) = buf else {
        unreachable!()
    };
    let fold = match p8.terminal {
        lance_graph_mask_risc::Terminal::GroupReduce { fold, .. } => fold,
        other => panic!("Q8 lowered onto {other:?}"),
    };
    normalize_group_sink(fold, &mut g8);
    report(
        "Q8",
        "delivery: GROUP BY message.mailbox COUNT (20k groups)",
        ms,
        o8_ms,
        format!("{} groups", g8.len()),
    );
    assert_eq!(g8, o8, "Q8 deliveries per mailbox");

    // -- documents -----------------------------------------------------
    let doc_live = live_mask(document);
    let doc_masks: [&[u64]; 1] = [&doc_live];
    let doc_lanes = document.lanes();
    let docp = planes(document, &doc_masks, &doc_lanes);
    let docd = d["document"];
    let (doc_ticket, folder) = (docd.u32("ticket"), docd.u32("folder"));
    let invoices = code("document", "folder", "invoices");
    let open = code("ticket", "status", "open");
    let tk_lanes = [ticket.lane(0)];
    let fgn_t = Foreign {
        planes: &[],
        lanes: &tk_lanes,
    };
    let q9 = Query {
        filter: Filter::and([
            Filter::plane(Mask(0)),
            Filter::cmp(Col(1), Cmp::EqU32(invoices)),
            Filter::eq_u32_via(Col(0), ForeignLane(0), open),
        ]),
        agg: Agg::Count,
    };
    let p9 = lower(&q9).expect("lower q9");
    let lo = Instant::now();
    let o9 = (0..docd.rows())
        .filter(|&i| folder[i] == 1 && status[doc_ticket[i] as usize] == 0)
        .count();
    let o9_ms = lo.elapsed().as_secs_f64() * 1e3;
    let (v, _, ms) = run(&p9, &docp, &fgn_t, reps, || OutBuf::None);
    report(
        "Q9",
        "documents: invoices attached to open tickets (fk join)",
        ms,
        o9_ms,
        count(&v),
    );
    assert_eq!(count(&v), o9, "Q9 invoices on open tickets");

    // -- binding refuses what the codebook does not hold ---------------
    let bad = table("ticket")
        .where_eq("status", "escalated")
        .count()
        .bind(&cat);
    assert_eq!(
        bad,
        Err(BindError::UnknownValue {
            field: "status".into(),
            value: "escalated".into()
        }),
        "unknown literal must be refused, never minted"
    );
    let bad = table("ticket").where_eq("subject", "x").count().bind(&cat);
    assert!(
        matches!(bad, Err(BindError::UnknownField { .. })),
        "unknown field refused"
    );
    println!("bind: unknown literal and unknown field refused, nothing minted");
    println!("ALL ANSWERS MATCH THE ORACLE");
}
