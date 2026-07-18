// Session persistence - kept for future use
#[allow(dead_code)]
pub struct Session { pub id: String }
#[allow(dead_code)]
impl Session {
    pub fn new() -> Self { Self { id: String::new() } }
}
