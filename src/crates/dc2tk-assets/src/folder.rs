//! Files in memory, standing for folders the converters would write and read back, named as
//! this system's file system names them: on Windows a name differing only in case, or in trailing
//! dots and spaces, is the same file, which keeps the name it was first written with.

/// A folder's files by path ("/" between parts), in the order they were first written.
#[derive(Clone, Debug, Default)]
pub struct Folder {
    files: Vec<(String, Vec<u8>)>,
}

/// The name the file system finds a path by.
fn key(path: &str) -> String {
    if cfg!(windows) {
        path.split('/').map(|part| part.trim_end_matches(['.', ' ']).to_lowercase()).collect::<Vec<_>>().join("/")
    } else {
        path.to_string()
    }
}

impl Folder {
    pub fn new() -> Folder {
        Folder::default()
    }

    fn position(&self, path: &str) -> Option<usize> {
        let wanted = key(path);
        self.files.iter().position(|(name, _)| key(name) == wanted)
    }

    /// Writes a file, over the one the path names if there is one.
    pub fn write(&mut self, path: &str, data: Vec<u8>) {
        match self.position(path) {
            Some(at) => self.files[at].1 = data,
            None => self.files.push((path.to_string(), data)),
        }
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.position(path).map(|at| self.files[at].1.as_slice())
    }

    pub fn is_file(&self, path: &str) -> bool {
        self.position(path).is_some()
    }

    /// The files as written: (path as first written, bytes).
    pub fn files(&self) -> &[(String, Vec<u8>)] {
        &self.files
    }

    pub fn files_mut(&mut self) -> &mut [(String, Vec<u8>)] {
        &mut self.files
    }

    /// The files sorted by their parts, ignoring case on Windows (as crate::sorted_paths sorts a folder's).
    pub fn sorted(&self) -> Vec<(String, Vec<u8>)> {
        let mut files = self.files.clone();
        if cfg!(windows) {
            files
                .sort_by_cached_key(|(path, _)| path.to_lowercase().split('/').map(str::to_string).collect::<Vec<_>>());
        } else {
            files.sort_by(|a, b| a.0.split('/').cmp(b.0.split('/')));
        }
        files
    }
}
