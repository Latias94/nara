use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn adr_directory() -> PathBuf {
    repository_root().join("docs/architecture/adr")
}

fn adr_id_from_filename(filename: &str) -> Option<&str> {
    let (id, rest) = filename.split_once('-')?;
    (id.len() == 4 && id.bytes().all(|byte| byte.is_ascii_digit()) && rest.ends_with(".md"))
        .then_some(id)
}

fn adr_files() -> Vec<PathBuf> {
    let mut files = fs::read_dir(adr_directory())
        .expect("ADR directory must be readable")
        .map(|entry| entry.expect("ADR directory entry must be readable").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(adr_id_from_filename)
                .is_some()
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn adr_status(document: &str) -> &str {
    let statuses = document
        .lines()
        .filter_map(|line| line.strip_prefix("**Status**:"))
        .map(str::trim)
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 1, "ADR must contain exactly one status");
    statuses[0]
}

fn markdown_targets(document: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut remainder = document;
    while let Some(start) = remainder.find("](") {
        let after = &remainder[start + 2..];
        let Some(end) = after.find(')') else {
            break;
        };
        targets.push(after[..end].trim().trim_matches(['<', '>']));
        remainder = &after[end + 1..];
    }
    targets
}

fn assert_anchor_paths_exist(cell: &str) {
    let mut fragments = cell.split('`');
    while let Some(_outside) = fragments.next() {
        let Some(anchor) = fragments.next() else {
            break;
        };
        let path = anchor
            .split('#')
            .next()
            .expect("anchor must contain a path");
        assert!(
            repository_root().join(path).exists(),
            "ledger anchor does not exist: {anchor}"
        );
    }
}

#[test]
fn adr_files_have_unique_ids_and_required_sections() {
    let allowed_statuses = ["Proposed", "Accepted", "Superseded", "Rejected"];
    let mut ids = BTreeSet::new();

    for path in adr_files() {
        let filename = path.file_name().unwrap().to_str().unwrap();
        let id = adr_id_from_filename(filename).unwrap();
        assert!(ids.insert(id.to_owned()), "duplicate ADR ID {id}");

        let document = read(&path);
        assert!(
            document.starts_with(&format!("# ADR {id}:")),
            "{filename} H1 must match its filename ID"
        );
        assert!(
            allowed_statuses.contains(&adr_status(&document)),
            "{filename} has an invalid decision status"
        );
        assert_eq!(
            document
                .lines()
                .filter(|line| line.starts_with("**Date**:"))
                .count(),
            1,
            "{filename} must contain exactly one date"
        );
        for heading in [
            "## Context",
            "## Decision",
            "## Alternatives Considered",
            "## Success Metrics",
            "## Risks and Mitigations",
        ] {
            assert!(document.contains(heading), "{filename} lacks {heading}");
        }

        if id >= "0057" {
            assert!(
                document.contains("## Consequences"),
                "new ADR {filename} must state consequences"
            );
            assert!(
                document.matches("### Option ").count() >= 2,
                "new ADR {filename} must compare at least two options"
            );
        }
    }
}

#[test]
fn adr_readme_indexes_every_decision_once() {
    let readme = read(&adr_directory().join("README.md"));
    assert!(
        readme.contains("Accepted` means the project selected the decision")
            && readme.contains("Implemented` means repository evidence proves"),
        "README must separate decision acceptance from implementation evidence"
    );

    for path in adr_files() {
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            readme.matches(&format!("({filename})")).count(),
            1,
            "README must index {filename} exactly once"
        );
    }
}

#[test]
fn repository_relative_markdown_links_resolve() {
    let mut documents = adr_files();
    documents.extend([
        adr_directory().join("README.md"),
        adr_directory().join("implementation-status.md"),
        repository_root().join("docs/architecture/open-questions.md"),
        repository_root().join("docs/migrations/2026-07-engine-foundation.md"),
    ]);

    for source in documents {
        let document = read(&source);
        for target in markdown_targets(&document) {
            let path_target = target.split('#').next().unwrap_or_default();
            if path_target.is_empty()
                || target.starts_with('#')
                || target.contains("://")
                || !path_target.ends_with(".md")
            {
                continue;
            }
            let resolved = source.parent().unwrap().join(path_target);
            assert!(
                resolved.exists(),
                "{} contains unresolved link {target}",
                source.display()
            );
        }
    }
}

#[test]
fn implementation_ledger_obeys_evidence_invariants() {
    let files = adr_files();
    let decisions = files
        .iter()
        .map(|path| {
            let filename = path.file_name().unwrap().to_str().unwrap();
            let id = adr_id_from_filename(filename).unwrap().to_owned();
            (id, adr_status(&read(path)).to_ascii_lowercase())
        })
        .collect::<BTreeMap<_, _>>();
    let ledger = read(&adr_directory().join("implementation-status.md"));
    let mut seen = BTreeSet::new();

    for line in ledger.lines().filter(|line| line.starts_with("| [")) {
        let cells = line
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        assert_eq!(
            cells.len(),
            10,
            "ledger rows must follow the declared schema"
        );

        let id = cells[0]
            .strip_prefix('[')
            .and_then(|value| value.split_once(']'))
            .map(|(id, _)| id)
            .expect("ledger ADR cell must start with a link");
        assert!(seen.insert(id.to_owned()), "duplicate ledger ADR {id}");
        assert_eq!(
            decisions.get(id).map(String::as_str),
            Some(cells[1]),
            "ledger decision status must mirror ADR {id}"
        );

        match cells[2] {
            "not-started" => {
                assert_eq!(cells[5], "-");
                assert_eq!(cells[6], "-");
                assert_ne!(cells[7], "-");
                assert_ne!(cells[8], "-");
            }
            "partial" => {
                assert_ne!(cells[5], "-");
                assert_ne!(cells[6], "-");
                assert_ne!(cells[7], "-");
                assert_ne!(cells[8], "-");
                assert_anchor_paths_exist(cells[5]);
                assert_anchor_paths_exist(cells[6]);
            }
            "implemented" => {
                assert_ne!(cells[5], "-");
                assert_ne!(cells[6], "-");
                assert_eq!(cells[7], "-");
                assert_eq!(cells[8], "-");
                assert_anchor_paths_exist(cells[5]);
                assert_anchor_paths_exist(cells[6]);
            }
            "superseded" => {}
            state => panic!("invalid implementation state {state}"),
        }
    }
}

#[test]
fn open_questions_are_trigger_based_and_unresolved() {
    let document = read(&repository_root().join("docs/architecture/open-questions.md"));
    assert!(
        !document.contains("Resolved in") && !document.contains("Resolved by"),
        "open questions must not retain resolved implementation history"
    );

    let questions = document.split("\n## OQ-").skip(1).collect::<Vec<_>>();
    assert!(!questions.is_empty());
    for question in questions {
        for field in [
            "- **Status**: open",
            "- **Owner**:",
            "- **Trigger**:",
            "- **Related ADRs**:",
            "- **Question**:",
        ] {
            assert!(question.contains(field), "open question lacks {field}");
        }
    }
}
