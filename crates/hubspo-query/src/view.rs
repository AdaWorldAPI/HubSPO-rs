//! D-QRY-3 (saved-view half): a saved view as data, and one page of it in
//! sort order over Quack programs.
//!
//! A [`SavedView`] is what HubSpot stores for an index page: the object
//! type, filter groups, at most one sort key and the columns shown. Nothing
//! here owns or shares a view; who may use one, and which of its columns a
//! request may read, is decided at the output edge (`hubspo-compat`).
//!
//! ## Sorting without `ORDER BY`
//!
//! Quack keeps storage order and has no `ORDER BY`, so [`page`] finds a
//! page's records with ordinary programs and sorts only what it must:
//!
//! - **No sort key:** the kept records in storage order (`Agg::Rows`).
//! - **`I32` key:** the values at the page's first and last rank are found
//!   by bisection over the value domain, each step one `Agg::Count` with a
//!   threshold (≤ 33 programs per bound). Only the band between the two
//!   values is materialised and sorted: the page plus the ties at its ends.
//! - **Code key:** one `GroupReduce Count` over the code lane gives every
//!   code's count; only the codes that overlap the page are materialised.
//!   Codes sort by their numeric identity (the codebook's order).
//!
//! The order is total and fixed: by value, ties by record ordinal ascending
//! in both directions, and records without a value (a clear validity bit)
//! last in both directions, in storage order. The oracle in
//! `tests/view_oracle.rs` states the same order as a plain sort.
//!
//! **Cost.** Materialised rows are the page plus every record sharing a
//! value with the page's first or last record (I32), or every record of a
//! code that overlaps the page (code). A page inside one huge code or one
//! huge tie materialises that whole group; if a page-cost probe shows that
//! dominates, `ORDER BY` is the upstream ask, not before.

use crate::filter::{CompileError, FilterGroup, compile};
use lance_graph_quack::bind::{Binder, FieldKind};
use lance_graph_quack::{Agg, Cmp, Col, Filter, GroupAddr, GroupAgg, Query, normalize_group_sink};

/// The direction of a sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Smallest value first.
    Ascending,
    /// Largest value first.
    Descending,
}

/// A view's sort key: one property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sort {
    /// The property's internal name.
    pub property: String,
    /// The direction.
    pub direction: Direction,
}

/// A saved view: data only.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SavedView {
    /// The object type (`contacts`, `deals`, ...).
    pub object: String,
    /// The filter groups; none keeps every live record.
    pub groups: Vec<FilterGroup>,
    /// The sort key; `None` is storage order.
    pub sort: Option<Sort>,
    /// The properties shown, in display order.
    pub columns: Vec<String>,
}

/// Executes the programs [`page`] builds.
pub trait Runner {
    /// Why execution failed.
    type Error;
    /// Run an `Agg::Count` query.
    ///
    /// # Errors
    ///
    /// The executor's error.
    fn count(&mut self, query: &Query) -> Result<u64, Self::Error>;
    /// Run an `Agg::Rows` query; the kept ordinals, ascending.
    ///
    /// # Errors
    ///
    /// The executor's error.
    fn rows(&mut self, query: &Query) -> Result<Vec<usize>, Self::Error>;
    /// Run an `Agg::GroupReduce` query into a raw `i64` sink of `groups`
    /// slots (not normalised).
    ///
    /// # Errors
    ///
    /// The executor's error.
    fn groups(&mut self, query: &Query, groups: u32) -> Result<Vec<i64>, Self::Error>;
    /// The size of a code lane's domain (codes are `0..size`).
    fn code_domain(&self, col: Col) -> u32;
    /// The value of an `I32` lane at `row`.
    fn i32_at(&self, col: Col, row: usize) -> i32;
}

/// Why a page could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageError<E> {
    /// The view's filter did not compile.
    Compile(CompileError),
    /// The sort property does not exist on the object.
    UnknownSortProperty(String),
    /// Execution failed.
    Run(E),
}

impl<E> From<CompileError> for PageError<E> {
    fn from(e: CompileError) -> Self {
        PageError::Compile(e)
    }
}

/// The ordinals of `limit` records starting at rank `offset` of `view`, in
/// the view's order.
///
/// # Errors
///
/// See [`PageError`].
pub fn page<R: Runner>(
    view: &SavedView,
    binder: &dyn Binder,
    offset: usize,
    limit: usize,
    runner: &mut R,
) -> Result<Vec<usize>, PageError<R::Error>> {
    let filter = compile(&view.object, &view.groups, binder)?;
    let Some(sort) = &view.sort else {
        let rows = runner.rows(&rows_query(&filter)).map_err(PageError::Run)?;
        return Ok(rows.into_iter().skip(offset).take(limit).collect());
    };
    let table = binder
        .table(&view.object)
        .ok_or_else(|| PageError::Compile(CompileError::UnknownObject(view.object.clone())))?;
    let field = binder
        .field(table, &sort.property)
        .ok_or_else(|| PageError::UnknownSortProperty(sort.property.clone()))?;

    let (valued, nulls) = match field.validity {
        Some(valid) => (
            Filter::and([filter.clone(), Filter::plane(valid)]),
            Some(Filter::and([
                filter.clone(),
                Filter::negate(Filter::plane(valid)),
            ])),
        ),
        None => (filter.clone(), None),
    };
    let n_valued = usize::try_from(
        runner
            .count(&count_query(&valued))
            .map_err(PageError::Run)?,
    )
    .unwrap_or(usize::MAX);

    let end = offset.saturating_add(limit);
    let mut out = Vec::with_capacity(limit.min(n_valued.max(1)));
    if offset < n_valued {
        let r1 = end.min(n_valued);
        let ranked = match field.kind {
            FieldKind::I32 => i32_page(&valued, field.col, sort.direction, offset, r1, runner)?,
            FieldKind::Code => code_page(&valued, field.col, sort.direction, offset, r1, runner)?,
        };
        out.extend(ranked);
    }
    if let Some(nulls) = nulls
        && end > n_valued
    {
        let skip = offset.saturating_sub(n_valued);
        let take = end - n_valued.max(offset);
        let rows = runner.rows(&rows_query(&nulls)).map_err(PageError::Run)?;
        out.extend(rows.into_iter().skip(skip).take(take));
    }
    Ok(out)
}

fn count_query(filter: &Filter) -> Query {
    Query {
        filter: filter.clone(),
        agg: Agg::Count,
    }
}

fn rows_query(filter: &Filter) -> Query {
    Query {
        filter: filter.clone(),
        agg: Agg::Rows,
    }
}

fn with(filter: &Filter, col: Col, cmp: Cmp) -> Filter {
    Filter::and([filter.clone(), Filter::cmp(col, cmp)])
}

/// The value at rank `r` (0-based) among `valued` records in `direction`.
fn value_at<R: Runner>(
    valued: &Filter,
    col: Col,
    direction: Direction,
    r: usize,
    runner: &mut R,
) -> Result<i32, PageError<R::Error>> {
    let r = r as u64;
    let mut count = |cmp| {
        runner
            .count(&count_query(&with(valued, col, cmp)))
            .map_err(PageError::Run)
    };
    // Ascending: the smallest t with |{v ≤ t}| > r.
    // Descending: the largest t with |{v ≥ t}| > r.
    let (mut lo, mut hi) = (i64::from(i32::MIN), i64::from(i32::MAX));
    while lo < hi {
        match direction {
            Direction::Ascending => {
                let mid = lo + (hi - lo) / 2;
                if count(Cmp::LeI32(mid as i32))? > r {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            Direction::Descending => {
                let mid = lo + (hi - lo + 1) / 2;
                if count(Cmp::GeI32(mid as i32))? > r {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
        }
    }
    Ok(lo as i32)
}

fn i32_page<R: Runner>(
    valued: &Filter,
    col: Col,
    direction: Direction,
    r0: usize,
    r1: usize,
    runner: &mut R,
) -> Result<Vec<usize>, PageError<R::Error>> {
    let first = value_at(valued, col, direction, r0, runner)?;
    let last = value_at(valued, col, direction, r1 - 1, runner)?;
    let (before_cmp, band) = match direction {
        Direction::Ascending => (
            Cmp::LtI32(first),
            Filter::and([
                valued.clone(),
                Filter::cmp(col, Cmp::GeI32(first)),
                Filter::cmp(col, Cmp::LeI32(last)),
            ]),
        ),
        Direction::Descending => (
            Cmp::GtI32(first),
            Filter::and([
                valued.clone(),
                Filter::cmp(col, Cmp::LeI32(first)),
                Filter::cmp(col, Cmp::GeI32(last)),
            ]),
        ),
    };
    let before = runner
        .count(&count_query(&with(valued, col, before_cmp)))
        .map_err(PageError::Run)?;
    let mut band: Vec<(i32, usize)> = runner
        .rows(&rows_query(&band))
        .map_err(PageError::Run)?
        .into_iter()
        .map(|row| (runner.i32_at(col, row), row))
        .collect();
    match direction {
        Direction::Ascending => band.sort_unstable(),
        Direction::Descending => band.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1))),
    }
    let skip = r0 - usize::try_from(before).unwrap_or(usize::MAX).min(r0);
    Ok(band
        .into_iter()
        .skip(skip)
        .take(r1 - r0)
        .map(|(_, row)| row)
        .collect())
}

fn code_page<R: Runner>(
    valued: &Filter,
    col: Col,
    direction: Direction,
    r0: usize,
    r1: usize,
    runner: &mut R,
) -> Result<Vec<usize>, PageError<R::Error>> {
    let domain = runner.code_domain(col);
    let mut sink = runner
        .groups(
            &Query {
                filter: valued.clone(),
                agg: Agg::GroupReduce {
                    key: GroupAddr::Local(col),
                    agg: GroupAgg::Count,
                },
            },
            domain,
        )
        .map_err(PageError::Run)?;
    let present = normalize_group_sink(GroupAgg::Count.fold(), &mut sink);
    let count_of = |c: u32| -> usize {
        let g = c as usize;
        if present[g / 64] >> (g % 64) & 1 == 1 {
            usize::try_from(sink[g].unsigned_abs()).unwrap_or(usize::MAX)
        } else {
            0
        }
    };
    let codes: Box<dyn Iterator<Item = u32>> = match direction {
        Direction::Ascending => Box::new(0..domain),
        Direction::Descending => Box::new((0..domain).rev()),
    };
    let mut out = Vec::with_capacity(r1 - r0);
    let mut start = 0usize;
    for code in codes {
        if start >= r1 {
            break;
        }
        let n = count_of(code);
        let stop = start + n;
        if n > 0 && stop > r0 {
            let rows = runner
                .rows(&rows_query(&with(valued, col, Cmp::EqU32(code))))
                .map_err(PageError::Run)?;
            let skip = r0.saturating_sub(start);
            let take = r1.min(stop) - start.max(r0);
            out.extend(rows.into_iter().skip(skip).take(take));
        }
        start = stop;
    }
    Ok(out)
}

/// The view's filter, compiled; for callers that run their own programs
/// over the same records (a board, a count badge).
///
/// # Errors
///
/// The first [`CompileError`].
pub fn filter(view: &SavedView, binder: &dyn Binder) -> Result<Filter, CompileError> {
    compile(&view.object, &view.groups, binder)
}
