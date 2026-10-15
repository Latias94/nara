use nara::project_host::ProjectContentSnapshot;

fn change_document(snapshot: ProjectContentSnapshot) {
    snapshot.startup_scene().entities.clear();
}

fn main() {}
