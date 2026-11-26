#[test]
fn schema_derive_reports_contract_errors() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/*.rs");
    #[cfg(not(feature = "postcard"))]
    tests.compile_fail("tests/ui/native/*.rs");
}
