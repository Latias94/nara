//! Bounded installation relationships; neither artifact inspection nor executable authority.

use nara_tooling::{
    EditorSourceToolDesiredSelection as Desired, EditorSourceToolInstalledEntry as Entry,
    EditorSourceToolInstalledSelection, EditorSourceToolSelectionError as Error,
    EditorSourceToolWithdrawalMode,
};

use super::super::artifact_store;

pub(super) const MAX_ENTRIES: usize = 32;
pub(super) const MAX_EDGES: usize = 128;
pub(super) const MAX_REQUIREMENTS: usize = 16;
pub(super) const MAX_DIRECTORY_BYTES: usize = artifact_store::PREFIX.len() + 255;
const MAX_RECEIPT_BYTES: u64 = 4096;
const MAX_MANIFEST_BYTES: u64 = 65_536;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Graph {
    pub entries: Vec<Entry>,
    pub desired: Option<Desired>,
}

impl Graph {
    /// Checks recorded facts and relationships. Actual manifest declarations need inspection.
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.entries.len() > MAX_ENTRIES {
            return Err(Error::Graph);
        }
        let mut edge_count = 0;
        for entry in &self.entries {
            validate_entry(entry)?;
            edge_count += entry.requirements.len();
            if edge_count > MAX_EDGES {
                return Err(Error::Graph);
            }
        }

        let mut directories: Vec<String> = Vec::with_capacity(self.entries.len());
        for (index, entry) in self.entries.iter().enumerate() {
            if self.entries[..index]
                .iter()
                .any(|previous| previous.installed.installation == entry.installed.installation)
            {
                return Err(Error::Graph);
            }
            let directory = entry.installed.directory.to_ascii_lowercase();
            if directories.contains(&directory) {
                return Err(Error::Graph);
            }
            directories.push(directory);
        }

        // One bit per admitted member keeps reachability independent of input depth.
        let mut reachable = [0_u32; MAX_ENTRIES];
        for (index, entry) in self.entries.iter().enumerate() {
            for binding in &entry.requirements {
                let target = self.index(&binding.installation)?;
                if target == index || self.entries[target].installed.package != binding.package {
                    return Err(Error::Graph);
                }
                reachable[index] |= 1_u32 << target;
            }
        }
        for through in 0..self.entries.len() {
            let successors = reachable[through];
            for targets in reachable.iter_mut().take(self.entries.len()) {
                if *targets & (1_u32 << through) != 0 {
                    *targets |= successors;
                }
            }
        }
        if reachable
            .iter()
            .take(self.entries.len())
            .enumerate()
            .any(|(index, targets)| targets & (1_u32 << index) != 0)
        {
            return Err(Error::Graph);
        }
        if let Some(desired) = &self.desired {
            let selected = self.index(&desired.installed.installation)?;
            if self.entries[selected].installed != desired.installed {
                return Err(Error::Graph);
            }
        }
        Ok(())
    }

    /// Adds complete members atomically. A repeat compares bindings as a set, not caller order.
    pub(super) fn register(&self, additions: &[Entry]) -> Result<Self, Error> {
        self.validate()?;
        if additions.len() > MAX_ENTRIES {
            return Err(Error::Graph);
        }
        for entry in additions {
            validate_entry(entry)?;
        }
        let mut next = self.clone();
        let mut edge_count: usize = next
            .entries
            .iter()
            .map(|entry| entry.requirements.len())
            .sum();
        for addition in additions {
            if let Some(existing) = next
                .entries
                .iter()
                .find(|entry| entry.installed.installation == addition.installed.installation)
            {
                if existing.installed != addition.installed
                    || existing.requirements.len() != addition.requirements.len()
                    || !existing
                        .requirements
                        .iter()
                        .all(|binding| addition.requirements.contains(binding))
                {
                    return Err(Error::Graph);
                }
                continue;
            }
            edge_count += addition.requirements.len();
            if next.entries.len() == MAX_ENTRIES || edge_count > MAX_EDGES {
                return Err(Error::Graph);
            }
            next.entries.push(addition.clone());
        }
        next.validate()?;
        next.canonicalize();
        Ok(next)
    }

    /// Selects exact member facts; a missing requirement-free member may join the same snapshot.
    ///
    /// The caller must inspect a new member's actual manifest before using this convenience.
    pub(super) fn select(&self, desired: Desired) -> Result<Self, Error> {
        self.validate()?;
        validate_installed(&desired.installed)?;
        let existing = self
            .entries
            .iter()
            .find(|entry| entry.installed.installation == desired.installed.installation);
        if existing.is_some_and(|entry| entry.installed != desired.installed)
            || (existing.is_none() && self.entries.len() == MAX_ENTRIES)
        {
            return Err(Error::Graph);
        }
        let mut next = self.clone();
        if existing.is_none() {
            next.entries.push(Entry {
                installed: desired.installed.clone(),
                requirements: Vec::new(),
            });
        }
        next.desired = Some(desired);
        next.validate()?;
        next.canonicalize();
        Ok(next)
    }

    /// Disables the slot without removing any admitted member or relationship.
    #[must_use]
    pub(super) fn disabled(&self) -> Self {
        let mut next = self.clone();
        if let Some(desired) = &mut next.desired {
            desired.enabled = false;
        }
        next.canonicalize();
        next
    }

    /// Returns the selected member and every forward requirement, ordered by installation ID.
    pub(super) fn closure(&self, id: &[u8; 32]) -> Result<Vec<Entry>, Error> {
        self.validate()?;
        let mut included = 1_u32 << self.index(id)?;
        for _ in 0..self.entries.len() {
            let before = included;
            for (index, entry) in self.entries.iter().enumerate() {
                if included & (1_u32 << index) == 0 {
                    continue;
                }
                for binding in &entry.requirements {
                    included |= 1_u32 << self.index(&binding.installation)?;
                }
            }
            if included == before {
                break;
            }
        }
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(index, _)| included & (1_u32 << index) != 0)
            .map(|(_, entry)| entry.clone())
            .collect();
        canonicalize_entries(&mut entries);
        Ok(entries)
    }

    /// Withdraws the complete reverse closure, including disabled and nonselected dependents.
    /// Artifact availability is deliberately irrelevant; this operation grants no deletion rights.
    pub(super) fn withdraw(
        &self,
        id: &[u8; 32],
        mode: EditorSourceToolWithdrawalMode,
    ) -> Result<(Self, Vec<Entry>), Error> {
        self.validate()?;
        let mut affected = 1_u32 << self.index(id)?;
        for _ in 0..self.entries.len() {
            let before = affected;
            for (index, entry) in self.entries.iter().enumerate() {
                for binding in &entry.requirements {
                    if affected & (1_u32 << self.index(&binding.installation)?) != 0 {
                        affected |= 1_u32 << index;
                        break;
                    }
                }
            }
            if before == affected {
                break;
            }
        }
        if mode == EditorSourceToolWithdrawalMode::RejectDependents && affected.count_ones() > 1 {
            return Err(Error::RequiredDependents);
        }
        let mut next = Self::default();
        let mut withdrawn = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if affected & (1_u32 << index) != 0 {
                withdrawn.push(entry.clone());
            } else {
                next.entries.push(entry.clone());
            }
        }
        if let Some(desired) = &self.desired {
            if affected & (1_u32 << self.index(&desired.installed.installation)?) == 0 {
                next.desired = Some(desired.clone());
            }
        }
        next.canonicalize();
        canonicalize_entries(&mut withdrawn);
        Ok((next, withdrawn))
    }

    fn index(&self, id: &[u8; 32]) -> Result<usize, Error> {
        self.entries
            .iter()
            .position(|entry| &entry.installed.installation == id)
            .ok_or(Error::Graph)
    }

    fn canonicalize(&mut self) {
        canonicalize_entries(&mut self.entries);
    }
}

fn canonicalize_entries(entries: &mut [Entry]) {
    entries.sort_unstable_by_key(|entry| entry.installed.installation);
    for entry in entries {
        entry
            .requirements
            .sort_unstable_by(|left, right| left.package.cmp(&right.package));
    }
}

fn validate_entry(entry: &Entry) -> Result<(), Error> {
    if entry.requirements.len() > MAX_REQUIREMENTS {
        return Err(Error::Graph);
    }
    validate_installed(&entry.installed)?;
    for (index, binding) in entry.requirements.iter().enumerate() {
        if !valid_package(&binding.package)
            || binding.installation == [0; 32]
            || entry.requirements[..index]
                .iter()
                .any(|previous| previous.package == binding.package)
        {
            return Err(Error::Graph);
        }
    }
    Ok(())
}

fn validate_installed(installed: &EditorSourceToolInstalledSelection) -> Result<(), Error> {
    if !valid_package(&installed.package)
        || installed.installation == [0; 32]
        || !(1..=MAX_RECEIPT_BYTES).contains(&installed.receipt.length())
        || !(1..=MAX_MANIFEST_BYTES).contains(&installed.manifest.length())
    {
        return Err(Error::InvalidRecord);
    }
    if !artifact_store::validate_path(&installed.directory) {
        return Err(Error::InvalidPath);
    }
    Ok(())
}

fn valid_package(package: &str) -> bool {
    !package.is_empty()
        && package.len() <= 128
        && package
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && package.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use nara_fs::ContentDigest;
    use nara_tooling::EditorSourceToolRequirementBinding as Binding;

    use super::*;

    fn entry(id: u8, package: &str, requirements: &[(u8, &str)]) -> Entry {
        Entry {
            installed: EditorSourceToolInstalledSelection {
                package: package.into(),
                directory: format!("{}install-{id}", artifact_store::PREFIX).into(),
                installation: [id; 32],
                receipt: ContentDigest::from_parts(32, [id; 32]),
                manifest: ContentDigest::from_parts(64, [7; 32]),
            },
            requirements: requirements
                .iter()
                .map(|(id, package)| Binding {
                    package: (*package).into(),
                    installation: [*id; 32],
                })
                .collect(),
        }
    }

    fn desired(entry: &Entry, enabled: bool) -> Desired {
        Desired {
            installed: entry.installed.clone(),
            enabled,
        }
    }

    #[test]
    fn registration_is_additive_exact_and_canonical_independent_of_binding_order() {
        let a = entry(1, "test.a", &[]);
        let b = entry(2, "test.b", &[]);
        let c = entry(3, "test.c", &[(2, "test.b"), (1, "test.a")]);
        let original = Graph::default()
            .register(&[c.clone(), b.clone(), a.clone()])
            .unwrap();
        let mut reordered = c.clone();
        reordered.requirements.reverse();
        assert_eq!(
            original
                .entries
                .iter()
                .map(|entry| entry.installed.installation[0])
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            original.entries[2].requirements[0].package.as_ref(),
            "test.a"
        );
        assert_eq!(
            original
                .register(&[a.clone(), reordered, c.clone()])
                .unwrap(),
            original
        );
        assert_eq!(Graph::default().register(&[a, c, b]).unwrap(), original);

        for changed in 0..6 {
            let mut conflict = original.entries[2].clone();
            match changed {
                0 => conflict.installed.package = "other".into(),
                1 => {
                    conflict.installed.directory = format!("{}other", artifact_store::PREFIX).into()
                }
                2 => conflict.installed.receipt = ContentDigest::from_parts(32, [9; 32]),
                3 => conflict.installed.manifest = ContentDigest::from_parts(64, [9; 32]),
                4 => conflict.requirements.clear(),
                _ => conflict.requirements[0].installation = [2; 32],
            }
            assert_eq!(original.register(&[conflict]), Err(Error::Graph));
        }
    }

    #[test]
    fn desired_selection_is_exact_and_missing_members_only_get_empty_requirements() {
        let a = entry(1, "test.a", &[]);
        let b = entry(2, "test.b", &[(1, "test.a")]);
        let graph = Graph::default().register(&[b.clone(), a.clone()]).unwrap();
        let selected = graph.select(desired(&b, true)).unwrap();
        assert_eq!(selected.entries, graph.entries);
        let mut wrong = desired(&b, true);
        wrong.installed.manifest = ContentDigest::from_parts(64, [9; 32]);
        assert_eq!(selected.select(wrong), Err(Error::Graph));
        let disabled = selected.disabled();
        assert_eq!(disabled.entries, graph.entries);
        assert_eq!(disabled.desired, Some(desired(&b, false)));
        assert_eq!(Graph::default().disabled(), Graph::default());

        let c = entry(3, "test.c", &[]);
        let expanded = disabled.select(desired(&c, true)).unwrap();
        assert_eq!(expanded.entries.len(), 3);
        assert!(expanded.entries[2].requirements.is_empty());
        let invalid = Graph {
            entries: vec![a],
            desired: Some(desired(&b, false)),
        };
        assert_eq!(invalid.validate(), Err(Error::Graph));
    }

    #[test]
    fn forward_closure_and_disabled_reverse_dependents_bind_exact_same_release_installations() {
        let a1 = entry(1, "test.a", &[]);
        let mut a2 = entry(2, "test.a", &[]);
        a2.installed.receipt = a1.installed.receipt;
        let b = entry(3, "test.b", &[(1, "test.a")]);
        let c = entry(4, "test.c", &[(3, "test.b")]);
        let graph = Graph::default()
            .register(&[c.clone(), a2.clone(), b.clone(), a1.clone()])
            .unwrap()
            .select(desired(&c, true))
            .unwrap()
            .disabled();
        assert_eq!(
            graph.closure(&[4; 32]).unwrap(),
            [a1.clone(), b.clone(), c.clone()]
        );
        assert_eq!(graph.closure(&[2; 32]).unwrap(), [a2.clone()]);
        assert_eq!(
            graph.withdraw(&[1; 32], EditorSourceToolWithdrawalMode::RejectDependents),
            Err(Error::RequiredDependents)
        );
        let before = graph.clone();
        let (next, removed) = graph
            .withdraw(&[1; 32], EditorSourceToolWithdrawalMode::Cascade)
            .unwrap();
        assert_eq!(graph, before);
        assert_eq!(removed, [a1, b, c]);
        assert_eq!(next.entries, [a2]);
        assert_eq!(next.desired, None);
        next.validate().unwrap();
        assert_eq!(graph.closure(&[9; 32]), Err(Error::Graph));
        assert_eq!(
            graph.withdraw(&[9; 32], EditorSourceToolWithdrawalMode::Cascade),
            Err(Error::Graph)
        );
    }

    #[test]
    fn dropping_a_successor_and_later_registration_do_not_refresh_an_earlier_graph_review() {
        let a = entry(1, "test.a", &[]);
        let b = entry(2, "test.b", &[(1, "test.a")]);
        let original = Graph::default().register(&[b.clone(), a.clone()]).unwrap();
        let reviewed = original
            .withdraw(&[1; 32], EditorSourceToolWithdrawalMode::Cascade)
            .unwrap();
        drop(reviewed);
        assert_eq!(original.entries, [a.clone(), b.clone()]);
        let c = entry(3, "test.c", &[(2, "test.b")]);
        let current = original.register(&[c.clone()]).unwrap();
        assert_ne!(original, current);
        assert_eq!(
            original
                .withdraw(&[1; 32], EditorSourceToolWithdrawalMode::Cascade)
                .unwrap()
                .1,
            [a.clone(), b.clone()]
        );
        assert_eq!(
            current
                .withdraw(&[1; 32], EditorSourceToolWithdrawalMode::Cascade)
                .unwrap()
                .1,
            [a, b, c]
        );
    }

    #[test]
    fn withdrawal_keeps_an_unaffected_desired_slot_and_removes_no_forward_requirements() {
        let a = entry(1, "test.a", &[]);
        let b = entry(2, "test.b", &[(1, "test.a")]);
        let c = entry(3, "test.c", &[]);
        for enabled in [true, false] {
            let graph = Graph::default()
                .register(&[a.clone(), b.clone(), c.clone()])
                .unwrap()
                .select(desired(&c, enabled))
                .unwrap();
            let (next, removed) = graph
                .withdraw(&[2; 32], EditorSourceToolWithdrawalMode::RejectDependents)
                .unwrap();
            assert_eq!(removed, [b.clone()]);
            assert_eq!(next.entries, [a.clone(), c.clone()]);
            assert_eq!(next.desired, Some(desired(&c, enabled)));
        }
    }

    #[test]
    fn rejects_duplicate_ids_and_case_insensitive_artifact_directory_aliases() {
        let a = entry(1, "test.a", &[]);
        for directory in [
            ".nara/source-tools/artifacts/install-1",
            ".nara/source-tools/artifacts/INSTALL-1",
        ] {
            let mut b = entry(2, "test.b", &[]);
            b.installed.directory = directory.into();
            assert_eq!(
                Graph::default().register(&[a.clone(), b]),
                Err(Error::Graph),
                "{directory}"
            );
        }
        let mut duplicate = a.clone();
        duplicate.installed.directory = format!("{}other", artifact_store::PREFIX).into();
        assert_eq!(
            Graph {
                entries: vec![a.clone(), duplicate],
                desired: None
            }
            .validate(),
            Err(Error::Graph)
        );
        let mut sibling = entry(2, "test.b", &[]);
        sibling.installed.directory = format!("{}install-10", artifact_store::PREFIX).into();
        assert!(Graph::default().register(&[a, sibling]).is_ok());
    }

    #[test]
    fn rejects_every_directory_outside_the_fixed_store_and_single_ascii_name() {
        for directory in [
            "tools/install-1",
            "installed",
            ".nara",
            ".NARA",
            ".nara/source-tools",
            ".NARA/Source-Tools",
            ".nara/source-tools/install-1",
            ".nara/source-tools-extra",
            ".nara-extra",
            ".nara/source-tools/artifacts",
            ".nara/source-tools/artifacts/",
            ".NARA/source-tools/artifacts/install-1",
            ".nara/Source-Tools/artifacts/install-1",
            ".nara/source-tools/Artifacts/install-1",
            ".nara/source-tools/artifacts-extra/install-1",
            ".nara/source-tools/artifacts/install-1/child",
            ".nara/source-tools/artifacts/install-1/",
            ".nara/source-tools/artifacts//install-1",
            ".nara/source-tools/artifacts/.",
            ".nara/source-tools/artifacts/..",
            ".nara/source-tools/artifacts/../install-1",
            ".nara/source-tools/artifacts/install-1.",
            ".nara/source-tools/artifacts/install-1 ",
            ".nara/source-tools/artifacts/install:1",
            ".nara/source-tools/artifacts/CON",
            ".nara/source-tools/artifacts/工具",
        ] {
            let mut installation = entry(1, "test.a", &[]);
            installation.installed.directory = directory.into();
            assert_eq!(
                Graph::default().register(&[installation]),
                Err(Error::InvalidPath)
            );
        }
        for name in ["install-1", "INSTALL-1", "sample_tool.v2"] {
            let mut installation = entry(1, "test.a", &[]);
            installation.installed.directory = format!("{}{name}", artifact_store::PREFIX).into();
            Graph::default().register(&[installation]).unwrap();
        }
        let mut installation = entry(1, "test.a", &[]);
        installation.installed.directory =
            format!("{}{}", artifact_store::PREFIX, "a".repeat(255)).into();
        Graph::default().register(&[installation.clone()]).unwrap();
        installation.installed.directory =
            format!("{}{}", artifact_store::PREFIX, "a".repeat(256)).into();
        assert_eq!(
            Graph::default().register(&[installation]),
            Err(Error::InvalidPath)
        );
    }

    #[test]
    fn source_namespace_includes_case_aliases_and_descendants_but_excludes_store_siblings() {
        for path in [
            ".nara/source-tools/artifacts",
            ".NARA/SOURCE-TOOLS/ARTIFACTS",
            ".nara/source-tools/artifacts/",
            ".nara/source-tools/artifacts/install-1/Tool.dll",
            ".NARA/source-tools/Artifacts/install-1/samples/工具.txt",
        ] {
            assert!(artifact_store::contains_path(path), "{path}");
        }
        for path in [
            ".nara",
            ".nara/source-tools",
            ".nara/source-tools/selection.json",
            ".nara/source-tools/artifacts-extra/install-1",
            ".nara/source-tools/artifacts.json",
            "scenes/imported.scene.json",
        ] {
            assert!(!artifact_store::contains_path(path), "{path}");
        }
    }

    #[test]
    fn rejects_missing_mismatched_duplicate_self_and_cyclic_requirements() {
        let a = entry(1, "test.a", &[]);
        for requirements in [
            vec![(9, "test.a")],
            vec![(1, "test.wrong")],
            vec![(1, "test.a"), (1, "test.a")],
            vec![(2, "test.b")],
            vec![(0, "test.a")],
        ] {
            assert_eq!(
                Graph::default().register(&[a.clone(), entry(2, "test.b", &requirements)]),
                Err(Error::Graph)
            );
        }
        assert_eq!(
            Graph::default().register(&[
                entry(1, "test.a", &[(2, "test.b")]),
                entry(2, "test.b", &[(1, "test.a")])
            ]),
            Err(Error::Graph)
        );
        assert_eq!(
            Graph::default().register(&[
                entry(1, "test.a", &[(2, "test.b")]),
                entry(2, "test.b", &[(3, "test.c")]),
                entry(3, "test.c", &[(1, "test.a")])
            ]),
            Err(Error::Graph)
        );
    }

    #[test]
    fn admits_exact_node_and_edge_bounds_but_rejects_the_first_excess() {
        let mut entries: Vec<_> = (1..=32)
            .map(|id| entry(id, &format!("test.p{id}"), &[]))
            .collect();
        let nodes = Graph::default().register(&entries).unwrap();
        assert_eq!(nodes.entries.len(), MAX_ENTRIES);
        assert_eq!(
            nodes.register(&[entry(33, "test.extra", &[])]),
            Err(Error::Graph)
        );
        assert_eq!(
            nodes.select(desired(&entry(33, "test.extra", &[]), true)),
            Err(Error::Graph)
        );
        entries.push(entry(33, "test.extra", &[]));
        assert_eq!(
            Graph {
                entries: entries.clone(),
                desired: None
            }
            .validate(),
            Err(Error::Graph)
        );
        assert_eq!(Graph::default().register(&entries), Err(Error::Graph));
        entries.pop();

        for index in 16..24 {
            entries[index].requirements = entries[..16]
                .iter()
                .map(|target| Binding {
                    package: target.installed.package.clone(),
                    installation: target.installed.installation,
                })
                .collect();
        }
        let exact = Graph::default().register(&entries).unwrap();
        assert_eq!(
            exact
                .entries
                .iter()
                .map(|entry| entry.requirements.len())
                .sum::<usize>(),
            MAX_EDGES
        );
        let package = entries[0].installed.package.clone();
        entries[24].requirements.push(Binding {
            package,
            installation: [1; 32],
        });
        assert_eq!(Graph::default().register(&entries), Err(Error::Graph));
        let mut excessive = entry(25, "test.p25", &[]);
        excessive.requirements = entries[..17]
            .iter()
            .map(|target| Binding {
                package: target.installed.package.clone(),
                installation: target.installed.installation,
            })
            .collect();
        assert_eq!(Graph::default().register(&[excessive]), Err(Error::Graph));
    }
}
