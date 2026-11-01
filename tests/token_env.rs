//! Only `Klaracase::from_env` reads the environment. These checks change a process-wide variable,
//! so they live in their own test binary, in one test.

use klaracase::{Error, Klaracase};

#[test]
fn only_from_env_reads_the_token_variable() {
    // SAFETY: this binary holds one test, so nothing else reads the environment meanwhile.
    unsafe { std::env::set_var("KLARACASE_API_TOKEN", "klc_env_token_placeholder") };

    for empty in ["", "   "] {
        let error = Klaracase::new(empty).unwrap_err();
        assert!(
            matches!(
                error,
                Error::Configuration {
                    code: "missing_token",
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(error.to_string().contains("KLARACASE_API_TOKEN"));
    }
    assert!(
        Klaracase::builder().build().is_err(),
        "no token set, no env read"
    );
    assert!(Klaracase::from_env().is_ok());
    assert!(Klaracase::new("klc_your_token_here").is_ok());

    // SAFETY: see above.
    unsafe { std::env::remove_var("KLARACASE_API_TOKEN") };
    let error = Klaracase::from_env().unwrap_err();
    assert!(matches!(error, Error::Configuration { .. }));
    assert!(error.to_string().contains("KLARACASE_API_TOKEN"));
}
