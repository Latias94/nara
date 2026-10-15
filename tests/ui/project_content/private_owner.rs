use nara::project_host::ProjectContentSnapshot;

fn take_owner(snapshot: ProjectContentSnapshot) {
    let _ = snapshot.inner;
}

fn main() {}
