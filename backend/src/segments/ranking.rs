//! Abgeleitete Wertung (RFC-0009, ADR-0015): Rang und Bestzeitenart werden nicht gespeichert, sondern bei
//! jeder Abfrage aus den gültigen Versuchen des veröffentlichten Stands berechnet.

use std::cmp::Ordering;
use std::collections::HashMap;

/// Ein gültiger Versuch mit den Feldern, nach denen gerankt wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valid {
    pub id: String,
    pub duration_ms: i64,
    pub start_ms: i64,
    pub activity_id: String,
    pub position: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    FirstBest,
    Improvement,
    Equaled,
}

impl RecordKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::FirstBest => "first_best",
            Self::Improvement => "improvement",
            Self::Equaled => "equaled",
        }
    }

    /// Zählt für `segment_success_count` (Erste Bestzeit oder Verbesserung).
    pub fn is_success(self) -> bool {
        matches!(self, Self::FirstBest | Self::Improvement)
    }
}

/// Abgeleitete Werte eines gültigen Versuchs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Derived {
    /// Platz in der Gesamtrangliste (1 = Bestzeit)
    pub rank: i64,
    pub record_kind: Option<RecordKind>,
    /// Zeitersparnis gegenüber der bis dahin kürzesten Dauer, nur bei `Improvement`
    pub improvement_ms: Option<i64>,
}

/// Rangliste: Dauer, tatsächlicher Start, Aktivitäts-ID, Versuchsposition (RFC-0009 „Segmentwertung“).
pub fn leaderboard_cmp(a: &Valid, b: &Valid) -> Ordering {
    (a.duration_ms, a.start_ms, &a.activity_id, a.position).cmp(&(b.duration_ms, b.start_ms, &b.activity_id, b.position))
}

/// Zeitliche Folge für die historischen Bestzeiten: Start, Aktivitäts-ID, Versuchsposition.
pub fn chronological_cmp(a: &Valid, b: &Valid) -> Ordering {
    (a.start_ms, &a.activity_id, a.position).cmp(&(b.start_ms, &b.activity_id, b.position))
}

/// Rang und Bestzeitenart aller gültigen Versuche eines Segments, nach Versuchs-ID.
pub fn derive(valid: &[Valid]) -> HashMap<String, Derived> {
    let mut by_rank: Vec<&Valid> = valid.iter().collect();
    by_rank.sort_by(|a, b| leaderboard_cmp(a, b));
    let mut out: HashMap<String, Derived> = by_rank
        .iter()
        .enumerate()
        .map(|(i, v)| (v.id.clone(), Derived { rank: i as i64 + 1, record_kind: None, improvement_ms: None }))
        .collect();

    let mut by_time: Vec<&Valid> = valid.iter().collect();
    by_time.sort_by(|a, b| chronological_cmp(a, b));
    let mut best: Option<i64> = None;
    for v in by_time {
        let d = out.get_mut(&v.id).expect("alle Versuche gerankt");
        match best {
            None => d.record_kind = Some(RecordKind::FirstBest),
            Some(b) if v.duration_ms < b => {
                d.record_kind = Some(RecordKind::Improvement);
                d.improvement_ms = Some(b - v.duration_ms);
            }
            Some(b) if v.duration_ms == b => d.record_kind = Some(RecordKind::Equaled),
            Some(_) => {}
        }
        best = Some(best.map_or(v.duration_ms, |b| b.min(v.duration_ms)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, duration_ms: i64, start_ms: i64, activity: &str, position: i64) -> Valid {
        Valid { id: id.into(), duration_ms, start_ms, activity_id: activity.into(), position }
    }

    #[test]
    fn history_of_records_follows_attempt_time_not_import_order() {
        // In Importreihenfolge: der späteste zuerst
        let all = [v("c", 90, 3000, "a3", 0), v("a", 100, 1000, "a1", 0), v("b", 95, 2000, "a2", 0), v("d", 95, 4000, "a4", 0)];
        let d = derive(&all);
        assert_eq!(d["a"].record_kind, Some(RecordKind::FirstBest));
        assert_eq!(d["a"].improvement_ms, None);
        assert_eq!(d["b"].record_kind, Some(RecordKind::Improvement));
        assert_eq!(d["b"].improvement_ms, Some(5));
        assert_eq!(d["c"].record_kind, Some(RecordKind::Improvement));
        assert_eq!(d["c"].improvement_ms, Some(5));
        // gleich schnell wie die damalige Bestzeit (90) wäre Einstellung; 95 ist langsamer: keine Art
        assert_eq!(d["d"].record_kind, None);
        assert_eq!((d["c"].rank, d["b"].rank, d["d"].rank, d["a"].rank), (1, 2, 3, 4));
    }

    #[test]
    fn equal_duration_is_equaled_and_ties_rank_by_start_activity_position() {
        let all = [v("x", 100, 1000, "a1", 0), v("y", 100, 2000, "a2", 0), v("z", 100, 2000, "a2", 1), v("w", 100, 2000, "a0", 5)];
        let d = derive(&all);
        assert_eq!(d["x"].record_kind, Some(RecordKind::FirstBest));
        assert_eq!(d["w"].record_kind, Some(RecordKind::Equaled));
        assert_eq!(d["y"].record_kind, Some(RecordKind::Equaled));
        // Start vor Aktivitäts-ID vor Position
        assert_eq!((d["x"].rank, d["w"].rank, d["y"].rank, d["z"].rank), (1, 2, 3, 4));
    }

    #[test]
    fn success_counts_first_best_and_improvement_only() {
        assert!(RecordKind::FirstBest.is_success());
        assert!(RecordKind::Improvement.is_success());
        assert!(!RecordKind::Equaled.is_success());
        assert!(derive(&[]).is_empty());
    }
}
