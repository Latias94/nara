fn main() -> Result<(), nara::prelude::AppRunError> {
    let mut app = nara::prelude::App::new();
    app.update()?;
    println!("nara runtime scaffold ready");
    Ok(())
}
