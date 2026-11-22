//! Geometrie: Distanz auf dem WGS84-Ellipsoid ohne Höhe (RFC-0008).

use geo::{Distance, Geodesic, Point as GeoPoint};

use crate::domain::track::e7_to_deg;

/// Distanz in Metern zwischen zwei Positionen (Grad mal 10^7), geodätisch auf WGS84.
pub fn distance_m(a: (i32, i32), b: (i32, i32)) -> f64 {
    if a == b {
        return 0.0;
    }
    let pa = GeoPoint::new(e7_to_deg(a.1), e7_to_deg(a.0));
    let pb = GeoPoint::new(e7_to_deg(b.1), e7_to_deg(b.0));
    Geodesic.distance(pa, pb)
}
