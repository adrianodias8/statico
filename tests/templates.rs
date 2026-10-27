//! Every built-in template, created the way `statico new` creates it, must
//! pass `statico check --strict` with no errors and no warnings.

use statico::new::{BUILTINS, new_project};

#[test]
fn every_builtin_template_builds_clean() {
    for t in BUILTINS {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(t.slug);
        new_project(dir.to_str().unwrap(), t.slug, false).unwrap();
        let r = statico::check::check(&dir, None, true).unwrap();
        assert!(
            r.ok && r.errors.is_empty() && r.warnings.is_empty(),
            "{}: errors {:?}, warnings {:?}",
            t.slug,
            r.errors,
            r.warnings
        );
        assert!(r.pages > 3, "{}: only {} pages", t.slug, r.pages);
    }
}
