use super::AuthoringReadScope;
use crate::{
    EditorDocumentQueryRejection, EditorEntityCursor, EditorEntityPage, EditorEntityQuery,
    EditorEntityQueryPosition,
};

impl AuthoringReadScope<'_> {
    pub(in crate::workspace) fn query_entities(
        self,
        query: EditorEntityQuery,
    ) -> Result<EditorEntityPage, EditorDocumentQueryRejection> {
        let (basis, expected_kind, next_record) = match query.position {
            EditorEntityQueryPosition::Start(basis) => (basis, None, 0),
            EditorEntityQueryPosition::Continue(cursor) => {
                self.owner().require(&cursor.owner)?;
                (cursor.basis, Some(cursor.kind), cursor.next_record)
            }
        };
        let source = self.resolve(&basis, expected_kind)?;
        let records = source.entities();
        let remaining = records
            .get(next_record..)
            .ok_or(EditorDocumentQueryRejection::InvalidContinuation)?;
        let scanned_records = remaining
            .len()
            .min(query.limits.max_scan_records())
            .min(query.limits.max_rows());
        let entities = remaining[..scanned_records]
            .iter()
            .map(|record| record.id.clone())
            .collect();
        let next_record = next_record + scanned_records;
        let next = (next_record < records.len()).then(|| EditorEntityCursor {
            owner: self.owner().clone(),
            basis: basis.clone(),
            kind: source.kind(),
            next_record,
        });
        Ok(EditorEntityPage {
            basis,
            entities,
            scanned_records,
            next,
        })
    }
}
