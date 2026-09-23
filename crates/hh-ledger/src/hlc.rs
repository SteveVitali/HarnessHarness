//! The hybrid logical clock stamped on every event of a **continuation or
//! child run** (§5a.3 `R-2.2.3⁰ᵇ`; ADR-0131 §5). A plain (non-lineage) run
//! carries no `hlc` — the envelope member stays absent there, so existing
//! byte-goldens never move (CC8).
//!
//! The stamp is `H<physical_ms:016x>.<logical:08x>.<node>`: fixed-width hex so
//! the string's lexicographic order equals the `(physical, logical, node)`
//! tuple order — a downstream consumer can sort continuation chains without a
//! decoder. `node` is the writer's holder identity — the cheapest stable
//! spelling of "which node wrote this" at this stage; cross-node ordering is
//! established by the physical/logical counters.
//!
//! Seeding: a run whose manifest carries `continued_from`, `forked_from` or
//! `parent_run_id` seeds its clock from the source run's head `hlc` (the
//! child's first stamp is causally after the anchor — `physical =
//! max(now, source.physical)`, `logical` continuing when physical ties).

use std::fmt;

/// One HLC value — `{physical_ms, logical, node}` rendered `H<ms>.<ctr>.<node>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hlc {
    /// The physical component (wall-ms).
    pub physical_ms: u64,
    /// The logical counter — increments when `physical` cannot advance.
    pub logical: u32,
    /// The node/writer identity (tiebreak; never empty).
    pub node: String,
}

impl Hlc {
    /// The canonical spelling — fixed-width hex, lexicographically ordered.
    pub fn render(&self) -> String {
        format!(
            "H{:016x}.{:08x}.{}",
            self.physical_ms, self.logical, self.node
        )
    }

    /// Parse a rendered stamp (`H<hex>.<hex>.<node>`); `None` on any other
    /// shape (absent on non-lineage runs — never an error).
    pub fn parse(s: &str) -> Option<Hlc> {
        let body = s.strip_prefix('H')?;
        let (phys, rest) = body.split_once('.')?;
        let (ctr, node) = rest.split_once('.')?;
        if node.is_empty() {
            return None;
        }
        Some(Hlc {
            physical_ms: u64::from_str_radix(phys, 16).ok()?,
            logical: u32::from_str_radix(ctr, 16).ok()?,
            node: node.to_string(),
        })
    }

    /// `tick(now)` — the next stamp, monotone in physical and never below the
    /// previous value (a backwards clock bumps `logical` instead of
    /// regressing — ADR-0131 §5's causal order).
    pub fn tick(&self, now_ms: u64) -> Hlc {
        if now_ms > self.physical_ms {
            Hlc {
                physical_ms: now_ms,
                logical: 0,
                node: self.node.clone(),
            }
        } else {
            Hlc {
                physical_ms: self.physical_ms,
                logical: self.logical.saturating_add(1),
                node: self.node.clone(),
            }
        }
    }

    /// Seed a continuation/child run's clock: causally after the source head
    /// (when the source was itself stamped) and at least `now`.
    pub fn seed(now_ms: u64, source_head: Option<&Hlc>, node: &str) -> Hlc {
        match source_head {
            Some(src) => {
                let physical = now_ms.max(src.physical_ms);
                let logical = if physical == src.physical_ms {
                    src.logical.saturating_add(1)
                } else {
                    0
                };
                Hlc {
                    physical_ms: physical,
                    logical,
                    node: node.to_string(),
                }
            }
            None => Hlc {
                physical_ms: now_ms,
                logical: 0,
                node: node.to_string(),
            },
        }
    }
}

impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_parse_roundtrip_and_order() {
        let a = Hlc {
            physical_ms: 1_700_000_000_000,
            logical: 3,
            node: "n-1".into(),
        };
        let s = a.render();
        assert_eq!(Hlc::parse(&s), Some(a.clone()));
        let b = a.tick(a.physical_ms); // same physical — logical advances
        assert!(b.logical == 4 && b.render() > s);
        let c = b.tick(a.physical_ms + 1);
        assert!(c.logical == 0 && c.render() > b.render());
        // Backwards clock never regresses.
        let d = c.tick(1);
        assert!(d.physical_ms == c.physical_ms && d.logical == c.logical + 1);
    }

    #[test]
    fn seed_is_causally_after_the_source_head() {
        let src = Hlc {
            physical_ms: 100,
            logical: 7,
            node: "n-0".into(),
        };
        let seeded = Hlc::seed(50, Some(&src), "n-1");
        assert!(seeded.physical_ms == 100 && seeded.logical == 8);
        let fresh = Hlc::seed(200, Some(&src), "n-1");
        assert!(fresh.physical_ms == 200 && fresh.logical == 0);
    }
}
