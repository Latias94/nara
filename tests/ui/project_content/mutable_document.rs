use nara::project_host::ProjectContentSnapshot;

fn change_document(snapshot: ProjectContentSnapshot) {
    snapshot.scene().entities.clear();
}

fn main() {}
