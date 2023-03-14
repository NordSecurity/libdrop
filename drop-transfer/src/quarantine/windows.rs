use std::{
    fs::File,
    io::{Error, ErrorKind, Result, Write},
    path::Path,
};

use super::PathExt;

impl PathExt for Path {
    fn quarantine(&self) -> Result<()> {
        if let Some(name) = self.file_name() {
            let mut name = name.to_os_string();

            name.push(":Zone.Identifier:$DATA");

            let mut f = File::create(self.with_file_name(name))?;

            write!(f, "[ZoneTransfer]\nZoneId=3")?;
        } else {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Cannot quarantine a directory",
            ));
        }

        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn write_zone_identifier() -> Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let path = file.path();
        let name = path.file_name();

        assert!(name.is_some());

        path.quarantine()?;

        let mut name = name.unwrap().to_os_string();

        name.push(":Zone.Identifier:$DATA");

        assert!(fs::read_to_string(path.with_file_name(name))? == "[ZoneTransfer]\nZoneId=3");

        Ok(())
    }
}
