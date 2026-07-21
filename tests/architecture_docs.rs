use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn adr_directory() -> PathBuf {
    repository_root().join("docs/architecture/adr")
}

fn read(path: &Path) -> String {
    normalize_newlines(
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
}

fn normalize_newlines(document: String) -> String {
    document.replace("\r\n", "\n")
}

fn repository_relative(path: &Path) -> String {
    path.strip_prefix(repository_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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

fn inline_code_values(document: &str) -> Vec<&str> {
    document.split('`').skip(1).step_by(2).collect()
}

fn section<'a>(document: &'a str, heading: &str) -> Option<&'a str> {
    let (_, after_heading) = document.split_once(heading)?;
    let end = after_heading.find("\n## ").unwrap_or(after_heading.len());
    Some(&after_heading[..end])
}

fn table_key_values(document: &str, heading: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for row in section(document, heading)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with('|'))
        .skip(2)
    {
        let mut columns = row.trim_matches('|').split('|').map(str::trim);
        let key = columns.next().unwrap().to_owned();
        let value = columns.next().unwrap().to_owned();
        assert!(
            values.insert(key.clone(), value).is_none(),
            "{heading} contains duplicate row {key}"
        );
    }
    values
}

fn expected_table<const N: usize>(rows: [(&str, &str); N]) -> BTreeMap<String, String> {
    rows.into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn frontmatter(document: &str) -> BTreeMap<String, String> {
    let mut lines = document.lines();
    if lines.next() != Some("---") {
        return BTreeMap::new();
    }
    lines
        .take_while(|line| *line != "---")
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| {
            (
                key.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            )
        })
        .collect()
}

fn metadata_field(document: &str, name: &str) -> Option<String> {
    let prefix = format!("**{name}**:");
    let mut lines = document.lines();
    while let Some(line) = lines.next() {
        let Some(first) = line.strip_prefix(&prefix) else {
            continue;
        };
        let mut value = first.trim().to_owned();
        for continuation in lines.by_ref() {
            if continuation.is_empty()
                || continuation.starts_with("**")
                || continuation.starts_with('#')
            {
                break;
            }
            value.push(' ');
            value.push_str(continuation.trim());
        }
        return Some(value);
    }
    None
}

fn relation_ids(value: Option<String>) -> BTreeSet<String> {
    let Some(value) = value else {
        return BTreeSet::new();
    };
    let bytes = value.as_bytes();
    let mut ids = BTreeSet::new();
    let mut offset = 0;
    while let Some(relative) = value[offset..].find("ADR ") {
        let start = offset + relative + 4;
        let end = start.saturating_add(4);
        if end <= bytes.len() && bytes[start..end].iter().all(u8::is_ascii_digit) {
            ids.insert(value[start..end].to_owned());
        }
        offset = end.min(bytes.len());
    }
    ids
}

#[derive(Clone, Debug)]
struct AdrRecord {
    filename: String,
    status: String,
    status_count: usize,
    document: String,
    refines: BTreeSet<String>,
    refined_by: BTreeSet<String>,
    supersedes: BTreeSet<String>,
    superseded_by: BTreeSet<String>,
    changed_by_successor: bool,
}

fn load_adrs() -> BTreeMap<String, AdrRecord> {
    adr_files()
        .into_iter()
        .map(|path| {
            let filename = path.file_name().unwrap().to_str().unwrap().to_owned();
            let id = adr_id_from_filename(&filename).unwrap().to_owned();
            let document = read(&path);
            let statuses = document
                .lines()
                .filter_map(|line| line.strip_prefix("**Status**:"))
                .map(str::trim)
                .collect::<Vec<_>>();
            let changed_by_successor = ["Implemented Slice", "Implemented Slices"]
                .into_iter()
                .filter_map(|field| metadata_field(&document, field))
                .any(|value| value.contains("RGF-"));
            let record = AdrRecord {
                filename,
                status: statuses.first().copied().unwrap_or_default().to_owned(),
                status_count: statuses.len(),
                refines: relation_ids(metadata_field(&document, "Refines")),
                refined_by: relation_ids(metadata_field(&document, "Refined By")),
                supersedes: relation_ids(metadata_field(&document, "Supersedes")),
                superseded_by: relation_ids(metadata_field(&document, "Superseded By")),
                changed_by_successor,
                document,
            };
            (id, record)
        })
        .collect()
}

#[derive(Clone, Debug)]
struct CatalogueEntry {
    id: String,
    filename: String,
}

fn parse_catalogue(document: &str) -> Vec<CatalogueEntry> {
    section(document, "## ADR Catalogue")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("- [ADR ")?;
            let id = rest.get(..4)?;
            let target_start = line.find("](")? + 2;
            let target_end = line[target_start..].find(')')? + target_start;
            Some(CatalogueEntry {
                id: id.to_owned(),
                filename: line[target_start..target_end].to_owned(),
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
struct LedgerRow {
    id: String,
    filename: String,
    decision: String,
    implementation: String,
    code_anchors: String,
    verification_anchors: String,
    remaining_gap: String,
    trigger: String,
    last_verified: String,
}

fn parse_ledger(document: &str) -> Vec<LedgerRow> {
    document
        .lines()
        .filter(|line| line.starts_with("| ["))
        .map(|line| {
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
            let target_start = cells[0]
                .find("](")
                .expect("ledger ADR cell must contain a link target")
                + 2;
            let target_end = cells[0][target_start..]
                .find(')')
                .expect("ledger ADR link target must close")
                + target_start;
            LedgerRow {
                id: id.to_owned(),
                filename: cells[0][target_start..target_end].to_owned(),
                decision: cells[1].to_owned(),
                implementation: cells[2].to_owned(),
                code_anchors: cells[5].to_owned(),
                verification_anchors: cells[6].to_owned(),
                remaining_gap: cells[7].to_owned(),
                trigger: cells[8].to_owned(),
                last_verified: cells[9].to_owned(),
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
struct PlanRecord {
    path: String,
    fields: BTreeMap<String, String>,
}

fn load_plans() -> BTreeMap<String, PlanRecord> {
    fs::read_dir(repository_root().join("docs/plans"))
        .expect("plan directory must be readable")
        .map(|entry| entry.expect("plan entry must be readable").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .map(|path| {
            let relative = repository_relative(&path);
            (
                relative.clone(),
                PlanRecord {
                    path: relative,
                    fields: frontmatter(&read(&path)),
                },
            )
        })
        .collect()
}

#[derive(Clone, Debug)]
struct ArchitectureRole {
    filename: String,
    role: String,
    state: String,
}

fn parse_architecture_roles(document: &str) -> Vec<ArchitectureRole> {
    section(document, "## Document Roles")
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with("| ["))
        .map(|line| {
            let cells = line
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect::<Vec<_>>();
            assert_eq!(
                cells.len(),
                3,
                "architecture role rows must have three cells"
            );
            let target_start = cells[0].find("](").expect("role document must be linked") + 2;
            let target_end = cells[0][target_start..]
                .find(')')
                .expect("role link must close")
                + target_start;
            ArchitectureRole {
                filename: cells[0][target_start..target_end].to_owned(),
                role: cells[1].to_owned(),
                state: cells[2].to_owned(),
            }
        })
        .filter(|entry| entry.filename != "nara-foundation.md")
        .collect()
}

fn top_level_design_documents() -> BTreeSet<String> {
    let authority = ["README.md", "nara-foundation.md", "open-questions.md"];
    fs::read_dir(repository_root().join("docs/architecture"))
        .expect("architecture directory must be readable")
        .map(|entry| entry.expect("architecture entry must be readable").path())
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|extension| extension == "md")
        })
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .filter(|filename| !authority.contains(&filename.as_str()))
        .collect()
}

fn table_capabilities(document: &str) -> BTreeSet<String> {
    section(document, "## Canonical Version-1 Vocabulary")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let cell = line.strip_prefix("| `")?;
            let end = cell.find('`')?;
            Some(cell[..end].to_owned())
        })
        .collect()
}

fn exact_capabilities(document: &str) -> BTreeSet<String> {
    let capability_section =
        section(document, "### Capabilities and Whole-Value Access").unwrap_or_default();
    let exact_paragraph = capability_section
        .split("\n\n")
        .find(|paragraph| paragraph.contains("contains exactly"))
        .unwrap_or_default();
    inline_code_values(exact_paragraph)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Debug)]
struct GovernanceSnapshot {
    adrs: BTreeMap<String, AdrRecord>,
    catalogue: Vec<CatalogueEntry>,
    ledger: Vec<LedgerRow>,
    plans: BTreeMap<String, PlanRecord>,
    architecture_readme: String,
    implementation_ledger: String,
    top_level_design_documents: BTreeSet<String>,
    architecture_roles: Vec<ArchitectureRole>,
    canonical_0045: BTreeSet<String>,
    canonical_0081: BTreeSet<String>,
    authority_documents: Vec<(String, String)>,
}

impl GovernanceSnapshot {
    fn load() -> Self {
        let adrs = load_adrs();
        let architecture_readme = read(&repository_root().join("docs/architecture/README.md"));
        let implementation_ledger = read(&adr_directory().join("implementation-status.md"));
        let canonical_0045 = table_capabilities(&adrs["0045"].document);
        let canonical_0081 = exact_capabilities(&adrs["0081"].document);
        Self {
            catalogue: parse_catalogue(&read(&adr_directory().join("README.md"))),
            ledger: parse_ledger(&implementation_ledger),
            plans: load_plans(),
            architecture_roles: parse_architecture_roles(&architecture_readme),
            top_level_design_documents: top_level_design_documents(),
            authority_documents: vec![
                (
                    "STRATEGY.md".to_owned(),
                    read(&repository_root().join("STRATEGY.md")),
                ),
                (
                    "docs/architecture/nara-foundation.md".to_owned(),
                    read(&repository_root().join("docs/architecture/nara-foundation.md")),
                ),
            ],
            adrs,
            architecture_readme,
            implementation_ledger,
            canonical_0045,
            canonical_0081,
        }
    }

    fn validate(&self) -> Result<(), String> {
        self.validate_sets_and_states()?;
        self.validate_evidence_anchors()?;
        self.validate_active_plan()?;
        self.validate_successor_relations()?;
        self.validate_capabilities()?;
        self.validate_proposed_authority()?;
        self.validate_architecture_roles()
    }

    fn validate_sets_and_states(&self) -> Result<(), String> {
        const DECISION_STATES: &[&str] = &["Proposed", "Accepted", "Superseded", "Rejected"];
        const IMPLEMENTATION_STATES: &[&str] =
            &["not-started", "partial", "implemented", "superseded"];

        let adr_ids = self.adrs.keys().cloned().collect::<BTreeSet<_>>();
        for (id, adr) in &self.adrs {
            if adr.status_count != 1 {
                return Err(format!("ADR {id} must contain exactly one decision status"));
            }
            if !DECISION_STATES.contains(&adr.status.as_str()) {
                return Err(format!(
                    "ADR {id} has invalid decision status {}",
                    adr.status
                ));
            }
        }

        let mut catalogue_ids = BTreeSet::new();
        for entry in &self.catalogue {
            if !catalogue_ids.insert(entry.id.clone()) {
                return Err(format!("duplicate catalogue ADR {}", entry.id));
            }
            let Some(adr) = self.adrs.get(&entry.id) else {
                return Err(format!("catalogue contains stale ADR {}", entry.id));
            };
            if entry.filename != adr.filename {
                return Err(format!(
                    "catalogue target for ADR {} is {}, expected {}",
                    entry.id, entry.filename, adr.filename
                ));
            }
        }
        if catalogue_ids != adr_ids {
            return Err("ADR file and catalogue sets differ".to_owned());
        }

        let mut ledger_ids = BTreeSet::new();
        for row in &self.ledger {
            if !ledger_ids.insert(row.id.clone()) {
                return Err(format!("duplicate ledger ADR {}", row.id));
            }
            let Some(adr) = self.adrs.get(&row.id) else {
                return Err(format!("ledger contains stale ADR {}", row.id));
            };
            if row.filename != adr.filename {
                return Err(format!(
                    "ledger target for ADR {} is {}, expected {}",
                    row.id, row.filename, adr.filename
                ));
            }
            if row.decision != adr.status.to_ascii_lowercase() {
                return Err(format!("ledger decision status differs for ADR {}", row.id));
            }
            if !IMPLEMENTATION_STATES.contains(&row.implementation.as_str()) {
                return Err(format!(
                    "ADR {} has invalid implementation status {}",
                    row.id, row.implementation
                ));
            }
            if (row.decision == "superseded") != (row.implementation == "superseded") {
                return Err(format!(
                    "ADR {} must keep decision and implementation supersession aligned",
                    row.id
                ));
            }
            match row.implementation.as_str() {
                "not-started" => {
                    require_equal(&row.code_anchors, "-", &row.id, "code anchors")?;
                    require_equal(
                        &row.verification_anchors,
                        "-",
                        &row.id,
                        "verification anchors",
                    )?;
                    require_gap(row)?;
                }
                "partial" => {
                    require_present(&row.code_anchors, &row.id, "code anchors")?;
                    require_present(&row.verification_anchors, &row.id, "verification anchors")?;
                    require_gap(row)?;
                }
                "implemented" => {
                    require_present(&row.code_anchors, &row.id, "code anchors")?;
                    require_present(&row.verification_anchors, &row.id, "verification anchors")?;
                    require_equal(&row.remaining_gap, "-", &row.id, "remaining gap")?;
                    require_equal(&row.trigger, "-", &row.id, "trigger")?;
                }
                "superseded" => {}
                _ => unreachable!(),
            }
        }
        if ledger_ids != adr_ids {
            return Err("ADR file and implementation-ledger sets differ".to_owned());
        }
        Ok(())
    }

    fn validate_evidence_anchors(&self) -> Result<(), String> {
        for row in &self.ledger {
            if !matches!(row.implementation.as_str(), "partial" | "implemented") {
                continue;
            }
            let validate_symbols = row.last_verified.contains("RGF-");
            for cell in [&row.code_anchors, &row.verification_anchors] {
                for anchor in inline_code_values(cell) {
                    validate_anchor(anchor, validate_symbols)
                        .map_err(|error| format!("ADR {}: {error}", row.id))?;
                }
            }
        }
        Ok(())
    }

    fn validate_active_plan(&self) -> Result<(), String> {
        let active = self
            .plans
            .values()
            .filter(|plan| {
                plan.fields
                    .get("execution_state")
                    .is_some_and(|state| state == "active")
            })
            .collect::<Vec<_>>();
        if active.len() != 1 {
            return Err(format!("expected one active plan, found {}", active.len()));
        }
        let active = active[0];
        let predecessor_path = active
            .fields
            .get("supersedes")
            .ok_or_else(|| "active plan lacks supersedes".to_owned())?;
        let predecessor = self
            .plans
            .get(predecessor_path)
            .ok_or_else(|| format!("active plan supersedes missing plan {predecessor_path}"))?;
        if predecessor.fields.get("superseded_by") != Some(&active.path) {
            return Err("active and predecessor plan supersession is not reciprocal".to_owned());
        }
        for (source, document) in [
            ("docs/architecture/README.md", &self.architecture_readme),
            (
                "docs/architecture/adr/implementation-status.md",
                &self.implementation_ledger,
            ),
        ] {
            let source_directory = Path::new(source).parent().unwrap();
            let references = markdown_targets(document)
                .into_iter()
                .filter_map(|target| normalize_markdown_target(source_directory, target))
                .filter(|target| target == &active.path)
                .count();
            if references != 1 {
                return Err(format!(
                    "{source} must point to the sole active plan exactly once"
                ));
            }
        }
        Ok(())
    }

    fn validate_successor_relations(&self) -> Result<(), String> {
        for (id, adr) in self.adrs.iter().filter(|(_, adr)| adr.changed_by_successor) {
            for target in &adr.refines {
                let target_adr = self
                    .adrs
                    .get(target)
                    .ok_or_else(|| format!("ADR {id} refines missing ADR {target}"))?;
                if !target_adr.refined_by.contains(id) {
                    return Err(format!("ADR {id} -> {target} refinement is not reciprocal"));
                }
            }
            for successor in &adr.refined_by {
                let successor_adr = self
                    .adrs
                    .get(successor)
                    .ok_or_else(|| format!("ADR {id} names missing refinement ADR {successor}"))?;
                if !successor_adr.refines.contains(id) {
                    return Err(format!(
                        "ADR {id} <- {successor} refinement is not reciprocal"
                    ));
                }
            }
            for target in &adr.supersedes {
                let target_adr = self
                    .adrs
                    .get(target)
                    .ok_or_else(|| format!("ADR {id} supersedes missing ADR {target}"))?;
                if !target_adr.superseded_by.contains(id) {
                    return Err(format!(
                        "ADR {id} -> {target} supersession is not reciprocal"
                    ));
                }
            }
            for successor in &adr.superseded_by {
                let successor_adr = self
                    .adrs
                    .get(successor)
                    .ok_or_else(|| format!("ADR {id} names missing successor ADR {successor}"))?;
                if !successor_adr.supersedes.contains(id) {
                    return Err(format!(
                        "ADR {id} <- {successor} supersession is not reciprocal"
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_capabilities(&self) -> Result<(), String> {
        if self.canonical_0045.is_empty() || self.canonical_0081.is_empty() {
            return Err("canonical-v1 capability vocabulary is empty".to_owned());
        }
        if self.canonical_0045 != self.canonical_0081 {
            return Err("ADR 0045 and ADR 0081 canonical-v1 capabilities differ".to_owned());
        }
        Ok(())
    }

    fn validate_proposed_authority(&self) -> Result<(), String> {
        let proposed = self
            .adrs
            .iter()
            .filter(|(_, adr)| adr.status == "Proposed")
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for (path, document) in &self.authority_documents {
            for paragraph in document.split("\n\n") {
                let lower = paragraph.to_ascii_lowercase();
                if !lower.contains("adr") {
                    continue;
                }
                for id in &proposed {
                    if paragraph.contains(id.as_str()) && !is_qualified_proposal_reference(&lower) {
                        return Err(format!(
                            "{path} cites Proposed ADR {id} as current authority"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_architecture_roles(&self) -> Result<(), String> {
        if !self
            .architecture_readme
            .contains("Design harnesses, appendices, and guides are non-normative")
            || !self
                .architecture_readme
                .contains("only an active plan may order implementation")
        {
            return Err("architecture index lacks its non-normative authority guard".to_owned());
        }
        let mut indexed = BTreeSet::new();
        for entry in &self.architecture_roles {
            if !indexed.insert(entry.filename.clone()) {
                return Err(format!(
                    "duplicate architecture role for {}",
                    entry.filename
                ));
            }
            let role = entry.role.to_ascii_lowercase();
            if !["harness", "appendix", "guide", "draft", "matrix"]
                .iter()
                .any(|class| role.contains(class))
            {
                return Err(format!(
                    "{} lacks a non-normative document role",
                    entry.filename
                ));
            }
            let state = entry.state.to_ascii_lowercase();
            if state.contains("needs rebaseline") && !state.contains("owns activation") {
                return Err(format!(
                    "{} needs rebaseline without an activation owner",
                    entry.filename
                ));
            }
        }
        if indexed != self.top_level_design_documents {
            return Err("top-level architecture document and role-index sets differ".to_owned());
        }
        Ok(())
    }
}

fn require_equal(value: &str, expected: &str, id: &str, field: &str) -> Result<(), String> {
    if value == expected {
        Ok(())
    } else {
        Err(format!("ADR {id} {field} must be {expected}"))
    }
}

fn require_present(value: &str, id: &str, field: &str) -> Result<(), String> {
    if value != "-" && !value.is_empty() {
        Ok(())
    } else {
        Err(format!("ADR {id} requires {field}"))
    }
}

fn require_gap(row: &LedgerRow) -> Result<(), String> {
    require_present(&row.remaining_gap, &row.id, "remaining gap")?;
    require_present(&row.trigger, &row.id, "trigger")
}

fn validate_anchor(anchor: &str, validate_symbol: bool) -> Result<(), String> {
    let (path, symbol) = anchor.split_once('#').unwrap_or((anchor, ""));
    let absolute = repository_root().join(path);
    if !absolute.exists() {
        return Err(format!("ledger anchor path does not exist: {anchor}"));
    }
    if !validate_symbol || symbol.is_empty() || !absolute.is_file() {
        return Ok(());
    }
    let terminal = symbol
        .rsplit("::")
        .next()
        .unwrap_or(symbol)
        .trim_matches(|character: char| !(character.is_ascii_alphanumeric() || character == '_'));
    if terminal.is_empty() || identifier_occurs(&read(&absolute), terminal) {
        Ok(())
    } else {
        Err(format!("ledger anchor symbol does not exist: {anchor}"))
    }
}

fn identifier_occurs(document: &str, identifier: &str) -> bool {
    document.match_indices(identifier).any(|(start, _)| {
        let before = document[..start].chars().next_back();
        let after = document[start + identifier.len()..].chars().next();
        !before.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
            && !after.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
    })
}

fn normalize_markdown_target(source_directory: &Path, target: &str) -> Option<String> {
    let target = target.split('#').next().unwrap_or_default();
    if target.is_empty() || target.contains("://") {
        return None;
    }
    let mut components = Vec::new();
    for component in source_directory.join(target).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop()?;
            }
            Component::Normal(component) => {
                components.push(component.to_string_lossy().into_owned())
            }
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }
    Some(components.join("/"))
}

fn is_qualified_proposal_reference(paragraph: &str) -> bool {
    [
        "proposed",
        "proposal",
        "trial",
        "candidate",
        "evidence",
        "not current authority",
        "not authoritative",
        "does not authorize",
        "owns acceptance",
        "until adr",
    ]
    .iter()
    .any(|qualifier| paragraph.contains(qualifier))
}

fn assert_rejected(baseline: &GovernanceSnapshot, mutate: impl FnOnce(&mut GovernanceSnapshot)) {
    let mut candidate = baseline.clone();
    mutate(&mut candidate);
    assert!(candidate.validate().is_err());
}

#[test]
fn adr_files_have_unique_ids_and_required_sections() {
    let snapshot = GovernanceSnapshot::load();
    for (id, adr) in &snapshot.adrs {
        assert!(
            adr.document.starts_with(&format!("# ADR {id}:")),
            "{} H1 must match its filename ID",
            adr.filename
        );
        assert_eq!(
            adr.document
                .lines()
                .filter(|line| line.starts_with("**Date**:"))
                .count(),
            1,
            "{} must contain exactly one date",
            adr.filename
        );
        for heading in [
            "## Context",
            "## Decision",
            "## Alternatives Considered",
            "## Success Metrics",
            "## Risks and Mitigations",
        ] {
            assert!(
                adr.document.contains(heading),
                "{} lacks {heading}",
                adr.filename
            );
        }
        if id.as_str() >= "0057" {
            assert!(
                adr.document.contains("## Consequences"),
                "new ADR {} must state consequences",
                adr.filename
            );
            assert!(
                adr.document.matches("### Option ").count() >= 2,
                "new ADR {} must compare at least two options",
                adr.filename
            );
        }
    }
}

#[test]
fn governance_snapshot_is_complete_and_consistent() {
    GovernanceSnapshot::load().validate().unwrap();
}

#[test]
fn u23_decision_matrix_preserves_independent_and_combined_verdicts() {
    const MATRIX_PATH: &str = "docs/knowledge/engineering/decisions/2026-07/\
2026-07-21T112729Z-rgf-u23-runtime-and-host-independent-decision-matrix-\
a5b3266847924dfc93667c72c8929550.md";

    let matrix = read(&repository_root().join(MATRIX_PATH));
    for required in [
        "| ADR 0084 executable Runtime | **Remain Proposed** |",
        "| ADR 0082 outer Host | **Remain Proposed** |",
        "| Combined topology | **Compatible bounded Trial** |",
        "| U20 admission | **Blocked** |",
        "already-compiled, Host-trusted",
        "universal Host, service registry, factory, or Runner SPI",
    ] {
        assert!(
            matrix.contains(required),
            "RGF-U23 decision matrix must retain `{required}`"
        );
    }

    assert_eq!(
        table_key_values(&matrix, "## Evidence Revisions"),
        expected_table([
            ("RGF-U3", "`4709689d50e1e5b4af41d062f7c308ef5bd6f377`"),
            ("RGF-U4", "`b6537579b3e48b11f36dca94fa28eb61b8262a3e`"),
            (
                "RGF-U5 corrected runtime",
                "`ff2e02a9ea087e32a00d90cde3b9e883dbc20c68`",
            ),
            ("RGF-U12", "`f341255559e201f32dbcb09888b16cd50fecdd85`"),
            ("RGF-U26", "`a2d695d6c58b8f8f21bb33aaf18fa27e18661a79`"),
            ("RGF-U24", "`5ddbf186712b6c829dc0134a9f41a0ac8250fa3e`"),
            ("RGF-U6", "`db511a780ad04e73940b72e6a4c3f0a48dbec70d`"),
            (
                "RGF-U13 final",
                "`5bc321d41aba59072a1f97ccc0473f91e0b2c161`",
            ),
            (
                "RGF-U17 final",
                "`0a87503b43e1d6abc8b23404789dafc1a7cfe22b`",
            ),
            ("RGF-U19", "`347015e8d9fd5d529b9dd5482ceaa02086f4615a`"),
            (
                "U23 review baseline",
                "`f7e5ee283e06ff156224b0f11fcc1df0c31284a3`",
            ),
        ])
    );
    assert_eq!(
        table_key_values(&matrix, "## ADR 0084 Runtime Metrics"),
        expected_table([
            ("Startup publication", "Pass"),
            ("Ownership handoff", "Pass"),
            ("App admission", "Pass"),
            ("Play execution", "Pass"),
            ("Driver parity", "Insufficient"),
            ("Driver authority", "Insufficient"),
            ("Exact step", "Pass"),
            ("Fault closure", "Pass"),
            ("Runtime isolation", "Insufficient"),
            ("Finite close", "Pass"),
            ("Stop-first workspace", "Pass"),
            ("API authority", "Pass"),
            ("Early ownership value", "Pass"),
        ])
    );
    assert_eq!(
        table_key_values(&matrix, "## ADR 0082 Host Metrics"),
        expected_table([
            ("Pre-mutation project rejection", "Pass"),
            ("Recipe coherence", "Pass"),
            ("Fresh plugin preparation", "Pass"),
            ("Early topology value", "Pass"),
            ("Runtime delegation", "Pass"),
            ("Parent lifetime", "Pass"),
            ("Cross-host parity", "Pass for Host merits"),
            ("Least privilege", "Pass"),
            ("Embedded path", "Pass"),
            (
                "Admitted external authority parity",
                "Pass in current scope",
            ),
        ])
    );
    assert_eq!(
        table_key_values(&matrix, "## Combined Runtime/Host Scenarios"),
        expected_table([
            ("Sequential RuntimeInstances", "Partial pass"),
            ("Overlapping RuntimeInstances", "Fail for drive semantics"),
            ("Process-global authority contention", "Fail"),
            ("Fresh-runtime reconstruction", "Partial pass"),
            ("Plan/World registry divergence", "Fail"),
        ])
    );

    let matrix_filename = Path::new(MATRIX_PATH)
        .file_name()
        .unwrap()
        .to_string_lossy();
    let snapshot = GovernanceSnapshot::load();
    let catalogue = read(&adr_directory().join("README.md"));
    for (id, adr) in [
        (
            "0082",
            "0082-process-host-authority-and-runtime-construction-topology.md",
        ),
        ("0084", "0084-executable-runtime-ownership-and-isolation.md"),
    ] {
        assert!(
            read(&adr_directory().join(adr)).contains(matrix_filename.as_ref()),
            "{adr} must cite the immutable RGF-U23 decision matrix"
        );
        assert_eq!(snapshot.adrs[id].status, "Proposed");
        let ledger = snapshot.ledger.iter().find(|row| row.id == id).unwrap();
        assert_eq!(ledger.decision, "proposed");
        assert!(ledger.last_verified.contains("RGF-U23"));
        assert!(
            ledger
                .verification_anchors
                .contains(matrix_filename.as_ref())
        );

        let prefix = format!("- [ADR {id}]({adr}):");
        let catalogue_entry = catalogue
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap();
        assert!(
            catalogue_entry.ends_with("(Proposed)"),
            "ADR {id} catalogue entry must remain Proposed"
        );
    }

    let foundation = read(&repository_root().join("docs/architecture/nara-foundation.md"));
    assert!(foundation.contains("RGF-U23 independently retained Proposed"));
    assert!(foundation.contains("compatible bounded Trial"));
}

#[test]
fn governance_parser_normalizes_windows_line_endings() {
    assert_eq!(
        normalize_newlines("first\r\n\r\nsecond\r\n".to_owned()),
        "first\n\nsecond\n"
    );
}

#[test]
fn governance_validator_rejects_set_and_status_drift() {
    let baseline = GovernanceSnapshot::load();
    assert_rejected(&baseline, |snapshot| {
        snapshot.adrs.remove("0001");
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.catalogue.remove(0);
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.ledger.remove(0);
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.ledger[0].filename = snapshot.ledger[1].filename.clone();
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.catalogue.push(snapshot.catalogue[0].clone());
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.catalogue.push(CatalogueEntry {
            id: "9999".to_owned(),
            filename: "9999-stale.md".to_owned(),
        });
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.catalogue[0].filename = "0001-stale.md".to_owned();
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.adrs.get_mut("0001").unwrap().status = "Trial".to_owned();
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.ledger[0].implementation = "accepted".to_owned();
    });
    let superseded_index = baseline
        .ledger
        .iter()
        .position(|row| row.decision == "superseded")
        .unwrap();
    assert_rejected(&baseline, |snapshot| {
        snapshot.ledger[superseded_index].implementation = "partial".to_owned();
    });
}

#[test]
fn governance_validator_rejects_evidence_and_authority_drift() {
    let baseline = GovernanceSnapshot::load();
    let partial_index = baseline
        .ledger
        .iter()
        .position(|row| row.decision == "accepted" && row.implementation == "partial")
        .unwrap();
    assert_rejected(&baseline, |snapshot| {
        snapshot.ledger[partial_index].remaining_gap = "-".to_owned();
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.ledger[partial_index].trigger = "-".to_owned();
    });
    let changed_index = baseline
        .ledger
        .iter()
        .position(|row| row.last_verified.contains("RGF-"))
        .unwrap();
    assert_rejected(&baseline, |snapshot| {
        snapshot.ledger[changed_index].code_anchors = "`missing/path.rs#Missing`".to_owned();
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.ledger[changed_index].code_anchors =
            "`src/lib.rs#DefinitelyMissingGovernanceSymbol`".to_owned();
    });
    assert_rejected(&baseline, |snapshot| {
        let capability = snapshot.canonical_0081.iter().next().unwrap().clone();
        snapshot.canonical_0081.remove(&capability);
    });
    assert_rejected(&baseline, |snapshot| {
        let active = snapshot
            .plans
            .values_mut()
            .find(|plan| {
                plan.fields
                    .get("execution_state")
                    .is_some_and(|state| state == "active")
            })
            .unwrap();
        active.fields.insert(
            "supersedes".to_owned(),
            "docs/plans/missing-plan.md".to_owned(),
        );
    });
    assert_rejected(&baseline, |snapshot| {
        let active = snapshot
            .plans
            .values()
            .find(|plan| {
                plan.fields
                    .get("execution_state")
                    .is_some_and(|state| state == "active")
            })
            .unwrap();
        let predecessor_path = active.fields["supersedes"].clone();
        snapshot
            .plans
            .get_mut(&predecessor_path)
            .unwrap()
            .fields
            .insert("superseded_by".to_owned(), "missing-plan.md".to_owned());
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot
            .adrs
            .get_mut("0081")
            .unwrap()
            .refines
            .remove("0045");
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.authority_documents.push((
            "synthetic-product.md".to_owned(),
            "ADR 0082 is current product authority.".to_owned(),
        ));
    });
    assert_eq!(
        baseline
            .ledger
            .iter()
            .find(|row| row.id == "0082")
            .unwrap()
            .implementation,
        "partial",
        "a Proposed ADR may retain bounded implementation evidence"
    );
}

#[test]
fn governance_validator_rejects_unclassified_or_unowned_drafts() {
    let baseline = GovernanceSnapshot::load();
    assert_rejected(&baseline, |snapshot| {
        snapshot.architecture_roles.remove(0);
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot
            .architecture_roles
            .push(snapshot.architecture_roles[0].clone());
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.architecture_roles[0].role = "Current architecture authority".to_owned();
    });
    assert_rejected(&baseline, |snapshot| {
        snapshot.architecture_roles[0].state = "Inactive and needs rebaseline".to_owned();
    });
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
