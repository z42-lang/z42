use super::*;

#[test]
fn console_input_terminal_and_absent_console_have_distinct_capabilities() {
    assert_eq!(
        classify(true, true, true),
        Capabilities {
            input: true,
            terminal: true
        }
    );
    assert_eq!(
        classify(true, true, false),
        Capabilities {
            input: true,
            terminal: false
        }
    );
    assert_eq!(
        classify(true, false, true),
        Capabilities {
            input: false,
            terminal: false
        }
    );
    assert_eq!(
        classify(false, true, true),
        Capabilities {
            input: false,
            terminal: false
        }
    );
    assert_eq!(
        classify(false, false, false),
        Capabilities {
            input: false,
            terminal: false
        }
    );
}

#[test]
fn platform_without_console_never_reports_terminal_input() {
    let caps = capabilities();
    assert!(!caps.terminal || caps.input);
    if !crate::pal::system::OS_CAPS.tty {
        assert_eq!(caps, classify(false, false, false));
    }
}
