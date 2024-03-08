use std::path::Path;
use crate::Parameters;

pub fn run(item: &Path, parameters: Parameters) -> Option<(bool, Option<String>)> {
    Some((true, None))
}

#[cfg(test)]
mod tests {
    
}
