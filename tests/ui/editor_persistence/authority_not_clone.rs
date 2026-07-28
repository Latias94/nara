use nara_tooling::EditorWorkspace;

fn main() {
    let (_, authority) = EditorWorkspace::new_hosted();
    let _duplicate = authority.clone();
}
