use std::fmt;

pub struct VersionInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub compiler: &'static str,
    pub compile_time: &'static str,
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "    Name: {}
    Version: {}
    Compiler: {}
    CompileTime: {}",
            self.name,
            self.version,
            self.compiler,
            self.compile_time
        )
    }
}