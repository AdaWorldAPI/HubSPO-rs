//! D-QRY-3 (board half): the pipeline board as two grouped Quack programs.
//!
//! HubSpot's board view shows, per pipeline stage, how many records sit in
//! it and the total (and probability-weighted) amount. That is a `GROUP BY
//! (pipeline, stage)` over the records a view's filter keeps, so it is two
//! [`Agg::GroupReduce`] programs over the composite address
//! `pipeline × stages + stage` ([`GroupAddr::Pair`]): one counts, one sums
//! the amount. Neither materialises a row or a composite key lane.
//!
//! Records without an amount are counted but not summed. A stage whose
//! records all lack an amount reports `amount: None`, never `0`: an empty sum
//! is not a zero sum (`normalize_group_sink`).
//!
//! Weighted amounts are exact: `amount × probability` in minor units ×
//! basis points (`i128`), so summing per record and multiplying the stage
//! total agree to the last unit. Rounding is the display's business.
//!
//! Not here yet (D-QRY-3's saved-view half): view definitions as data and
//! sorting. Quack has no `ORDER BY` (order is storage order), so a sorted
//! table view needs its own decision.

use lance_graph_quack::{Agg, Col, Filter, GroupAddr, GroupAgg, Mask, Query, normalize_group_sink};

/// Which lanes a board reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Board {
    /// The pipeline code lane (`u32`, `0..pipelines`).
    pub pipeline: Col,
    /// The stage code lane (`u32`, `0..stages`, numbered within a pipeline).
    pub stage: Col,
    /// Number of pipelines.
    pub pipelines: u32,
    /// Stage slots per pipeline (the largest pipeline's stage count).
    pub stages: u32,
    /// The amount lane (`i32`, minor units) and its validity plane when
    /// nullable.
    pub amount: Option<(Col, Option<Mask>)>,
}

/// Why a board cannot be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardError {
    /// `pipelines × stages` is zero or does not fit a group universe.
    GroupUniverse,
}

/// One board column: a stage of a pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    /// Pipeline code.
    pub pipeline: u32,
    /// Stage code within the pipeline.
    pub stage: u32,
    /// Records in the stage.
    pub count: u64,
    /// Sum of their amounts; `None` when none of them has an amount.
    pub amount: Option<i64>,
}

impl Board {
    /// The group universe `pipelines × stages`.
    ///
    /// # Errors
    ///
    /// [`BoardError::GroupUniverse`] if it is zero or overflows `u32`.
    pub fn groups(&self) -> Result<u32, BoardError> {
        match self.pipelines.checked_mul(self.stages) {
            Some(0) | None => Err(BoardError::GroupUniverse),
            Some(g) => Ok(g),
        }
    }

    fn addr(&self) -> GroupAddr {
        GroupAddr::Pair {
            hi: self.pipeline,
            lo: self.stage,
            stride: self.stages,
        }
    }

    /// Records per stage, over the records `filter` keeps. The caller runs it
    /// with an `Out::I64` sink of [`Board::groups`] slots.
    pub fn count_query(&self, filter: &Filter) -> Query {
        Query {
            filter: filter.clone(),
            agg: Agg::GroupReduce {
                key: self.addr(),
                agg: GroupAgg::Count,
            },
        }
    }

    /// Amount per stage over the kept records that have an amount; `None`
    /// when the board has no amount lane.
    pub fn amount_query(&self, filter: &Filter) -> Option<Query> {
        let (col, validity) = self.amount?;
        let filter = match validity {
            Some(valid) => Filter::and([filter.clone(), Filter::plane(valid)]),
            None => filter.clone(),
        };
        Some(Query {
            filter,
            agg: Agg::GroupReduce {
                key: self.addr(),
                agg: GroupAgg::SumI32(col),
            },
        })
    }

    /// The board's columns from the two executed sinks, in group order;
    /// stages no kept record reached are left out.
    ///
    /// Both sinks are normalised in place (`normalize_group_sink`).
    pub fn columns(&self, counts: &mut [i64], amounts: Option<&mut [i64]>) -> Vec<Column> {
        let present = normalize_group_sink(GroupAgg::Count.fold(), counts);
        let amount_present = match (self.amount, amounts) {
            (Some((col, _)), Some(sink)) => Some((
                normalize_group_sink(GroupAgg::SumI32(col).fold(), sink),
                sink,
            )),
            _ => None,
        };
        let bit = |mask: &[u64], g: usize| mask[g / 64] >> (g % 64) & 1 == 1;
        let stride = self.stages as usize;
        (0..counts.len())
            .filter(|&g| bit(&present, g))
            .map(|g| Column {
                pipeline: (g / stride) as u32,
                stage: (g % stride) as u32,
                count: counts[g].unsigned_abs(),
                amount: amount_present
                    .as_ref()
                    .and_then(|(mask, sink)| bit(mask, g).then(|| sink[g])),
            })
            .collect()
    }
}

/// `amount × probability` in minor units × basis points (exact). Divide by
/// 10,000 for minor units.
pub fn weighted(amount_minor: i64, probability_bp: u16) -> i128 {
    i128::from(amount_minor) * i128::from(probability_bp)
}
