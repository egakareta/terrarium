use terrarium::{JavaScriptRuntime, rquickjs};

#[test]
fn javascript_errors_are_returned_to_the_caller() {
    let runtime = JavaScriptRuntime::new().expect("JavaScript runtime should initialize");

    let error = runtime
        .run("throw new Error('script failed')")
        .expect_err("throwing JavaScript should fail the operation");

    assert!(matches!(error, rquickjs::Error::Exception));
    assert!(
        runtime
            .format_error(&error)
            .contains("Error: script failed")
    );
}
