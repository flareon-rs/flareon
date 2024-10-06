// inline(never) is added to make sure there is a separate frame for this
// function so that it can be used to find the start of the backtrace.
#[inline(never)]
pub(crate) fn __flareon_create_backtrace() -> Backtrace {
    let mut backtrace = Vec::new();
    let mut start = false;
    backtrace::trace(|frame| {
        let frame = StackFrame::from(frame);
        if start {
            backtrace.push(frame);
        } else if frame.symbol_name().contains("__flareon_create_backtrace") {
            // TODO does this work with strip = true? (probably not, in that case we should
            // return all frames instead)
            start = true;
        }

        true
    });

    Backtrace { frames: backtrace }
}

#[derive(Debug, Clone)]
pub(crate) struct Backtrace {
    frames: Vec<StackFrame>,
}

impl Backtrace {
    #[must_use]
    pub(crate) fn frames(&self) -> &[StackFrame] {
        &self.frames
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StackFrame {
    symbol_name: Option<String>,
    filename: Option<String>,
    lineno: Option<u32>,
    colno: Option<u32>,
}

impl StackFrame {
    #[must_use]
    pub(crate) fn symbol_name(&self) -> String {
        self.symbol_name
            .as_deref()
            .unwrap_or("<unknown>")
            .to_string()
    }

    #[must_use]
    pub(crate) fn location(&self) -> String {
        if let Some(filename) = self.filename.as_deref() {
            let mut s = filename.to_owned();

            if let Some(line_no) = self.lineno {
                s = format!("{s}:{line_no}");

                if let Some(col_no) = self.colno {
                    s = format!("{s}:{col_no}");
                }
            }

            s
        } else {
            "<unknown>".to_string()
        }
    }
}

impl From<&backtrace::Frame> for StackFrame {
    fn from(frame: &backtrace::Frame) -> Self {
        let mut symbol_name = None;
        let mut filename = None;
        let mut lineno = None;
        let mut colno = None;

        backtrace::resolve_frame(frame, |symbol| {
            if let Some(name) = symbol.name() {
                symbol_name = Some(name.to_string());
            }
            if let Some(file) = symbol.filename() {
                filename = Some(file.display().to_string());
            }
            if let Some(line) = symbol.lineno() {
                lineno = Some(line);
            }
            if let Some(col) = symbol.colno() {
                colno = Some(col);
            }
        });

        Self {
            symbol_name,
            filename,
            lineno,
            colno,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_backtrace() {
        let backtrace = __flareon_create_backtrace();
        assert!(
            !backtrace.frames().is_empty(),
            "Backtrace should have frames"
        );
    }

    #[test]
    fn test_stack_frame_symbol_name() {
        let frame = StackFrame {
            symbol_name: Some("test_symbol".to_string()),
            filename: None,
            lineno: None,
            colno: None,
        };
        assert_eq!(frame.symbol_name(), "test_symbol");
    }

    #[test]
    fn test_stack_frame_symbol_name_unknown() {
        let frame = StackFrame {
            symbol_name: None,
            filename: None,
            lineno: None,
            colno: None,
        };
        assert_eq!(frame.symbol_name(), "<unknown>");
    }

    #[test]
    fn test_stack_frame_location() {
        let frame = StackFrame {
            symbol_name: None,
            filename: Some("test_file.rs".to_string()),
            lineno: Some(42),
            colno: Some(7),
        };
        assert_eq!(frame.location(), "test_file.rs:42:7");
    }

    #[test]
    fn test_stack_frame_location_no_colno() {
        let frame = StackFrame {
            symbol_name: None,
            filename: Some("test_file.rs".to_string()),
            lineno: Some(42),
            colno: None,
        };
        assert_eq!(frame.location(), "test_file.rs:42");
    }

    #[test]
    fn test_stack_frame_location_unknown() {
        let frame = StackFrame {
            symbol_name: None,
            filename: None,
            lineno: None,
            colno: None,
        };
        assert_eq!(frame.location(), "<unknown>");
    }

    #[test]
    fn test_backtrace_frames() {
        let backtrace = Backtrace {
            frames: vec![StackFrame {
                symbol_name: Some("test_symbol".to_string()),
                filename: Some("test_file.rs".to_string()),
                lineno: Some(42),
                colno: Some(7),
            }],
        };
        assert_eq!(backtrace.frames().len(), 1);
        assert_eq!(backtrace.frames()[0].symbol_name(), "test_symbol");
    }
}
