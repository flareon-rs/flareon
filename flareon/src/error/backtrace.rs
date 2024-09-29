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
