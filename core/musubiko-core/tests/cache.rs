//! The status cache: what gets recomputed, and that cached answers equal fresh ones.

mod common;

use common::{Fixture, flow_fixture};
use musubiko_core::{Branch, CacheStats};

fn list(f: &Fixture, base: &str) -> (Vec<Branch>, CacheStats) {
    f.repo().branches_with_stats(base).unwrap()
}

fn summary(list: &[Branch]) -> Vec<String> {
    list.iter()
        .map(|b| {
            format!(
                "{} {:?} {} {} {}",
                b.name, b.status, b.ahead, b.behind, b.owner.email
            )
        })
        .collect()
}

#[test]
fn a_second_listing_computes_nothing_and_answers_the_same() {
    let f = flow_fixture();
    let (first, s1) = list(&f, "develop");
    assert_eq!(s1.hits, 0);
    assert_eq!(s1.computed as usize, first.len());

    let (second, s2) = list(&f, "develop");
    assert_eq!(
        s2,
        CacheStats {
            hits: first.len() as u32,
            computed: 0
        }
    );
    assert_eq!(summary(&first), summary(&second));
}

#[test]
fn only_branches_that_moved_are_recomputed() {
    let f = flow_fixture();
    list(&f, "develop");
    f.commit("o2.txt", "more\n", "more open work"); // HEAD is feature/open
    let (after, s) = list(&f, "develop");
    assert_eq!(s.computed, 1);
    let open = after.iter().find(|b| b.name == "feature/open").unwrap();
    assert_eq!(open.ahead, 2);
}

#[test]
fn moving_the_base_recomputes_against_it_only() {
    let f = flow_fixture();
    let (all, _) = list(&f, "develop");
    list(&f, "master");
    f.git(&["checkout", "-q", "develop"]);
    f.commit("dev.txt", "d\n", "develop moves on");
    let (_, s) = list(&f, "develop");
    // Everything compared with develop is new; the hotfix master half still compares with master.
    let against_master = all.iter().filter(|b| b.target == "master").count() as u32;
    assert_eq!(s.hits, against_master);
    assert_eq!(s.computed as usize, all.len() - against_master as usize);
    // master listing is untouched by develop moving, except branches whose own tip is develop now.
    let (_, s) = list(&f, "master");
    assert!(s.hits > 0);
}

#[test]
fn a_broken_cache_file_is_ignored() {
    let f = flow_fixture();
    let (fresh, _) = list(&f, "develop");
    let path = f.root.join(".git/musubiko/status-cache.json");
    assert!(path.exists(), "the cache lives in .git/musubiko");
    std::fs::write(&path, b"{ not json").unwrap();
    let (again, s) = list(&f, "develop");
    assert_eq!(s.hits, 0);
    assert_eq!(summary(&fresh), summary(&again));
    let (_, s) = list(&f, "develop");
    assert_eq!(s.computed, 0, "rewritten after the broken one was ignored");
}

#[test]
fn clearing_the_cache_starts_over() {
    let f = flow_fixture();
    list(&f, "develop");
    f.repo().clear_status_cache();
    let (_, s) = list(&f, "develop");
    assert_eq!(s.hits, 0);
}
