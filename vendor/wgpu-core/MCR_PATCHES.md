# mcr patches to wgpu-core 30.0.1

This directory is the crates.io release of `wgpu-core` 30.0.1, used through `[patch.crates-io]` in the
workspace `Cargo.toml`. Every difference from upstream is listed here; re-apply them on a wgpu upgrade, and drop
this directory once upstream makes them unnecessary.

## Files

- Removed `.cargo_vcs_info.json`, `Cargo.toml.orig` and `Cargo.lock`: packaging metadata, unused by a path dependency.
- Added `rustfmt.toml` (`disable_all_formatting = true`): keeps `cargo fmt --all` from reformatting upstream code.

## `src/track/metadata.rs`: clears and drops cost the members, not the largest index

Upstream clears and drops `resources` element by element up to the largest resource index ever tracked. Java's
loading leaves about 11,000 texture indices allocated, so every encoder's trackers paid that on `finish` and drop.
Against stock: Echoes of the Sift frame time 0.393 to 0.361 ms (8.2%), Medieval Immersed `finish` CPU 21% lower
(A/B in `openspec/changes/archive/*-mod-perf-3/design.md`).

```diff
--- a/src/track/metadata.rs
+++ b/src/track/metadata.rs
@@ -35,10 +35,21 @@
     }
 
     pub(super) fn clear(&mut self) {
-        self.resources.clear();
+        self.release_owned();
         self.owned.fill(false);
     }
 
+    /// Drops the members; the table keeps its length, so clearing and dropping cost the number of
+    /// members rather than the largest index ever tracked (mcr patch).
+    fn release_owned(&mut self) {
+        let resources = &mut self.resources;
+        for index in iterate_bitvec_indices(&self.owned) {
+            if let Some(slot) = resources.get_mut(index) {
+                *slot = None;
+            }
+        }
+    }
+
     /// Ensures a given index is in bounds for all arrays and does
     /// sanity checks of the presence of a refcount.
     ///
@@ -139,6 +150,16 @@
     }
 }
 
+impl<T: Clone> Drop for ResourceMetadata<T> {
+    fn drop(&mut self) {
+        self.release_owned();
+        strict_assert!(self.resources.iter().all(Option::is_none));
+        // SAFETY: members are exactly the `Some` elements, so all are `None` now; a broken
+        // invariant would only leak a reference.
+        unsafe { self.resources.set_len(0) };
+    }
+}
+
 /// A source of resource metadata.
 ///
 /// This is used to abstract over the various places
```

## `src/lock/ranked.rs`: `expect(unused)` becomes `allow(unused)`

A path dependency is not lint-capped like a registry one, so the unfulfilled `expect` warns (and fails
`clippy -D warnings`) in this build configuration.

```diff
--- a/src/lock/ranked.rs
+++ b/src/lock/ranked.rs
@@ -80,7 +80,7 @@
 /// For details, see [the module documentation][self].
 pub struct MutexGuard<'a, T> {
     inner: parking_lot::MutexGuard<'a, T>,
-    #[cfg_attr(not(miri), expect(unused))] // but `Drop` has important side effects
+    #[cfg_attr(not(miri), allow(unused))] // but `Drop` has important side effects
     saved: LockStateGuard,
 }
 
```
