#[derive(Debug, Clone, Copy)]
pub struct ConnectionDetails {
    pub domain: &'static str,
    pub login: &'static str,
    pub buildprefix: &'static str,
    pub rabbitprefix: &'static str,
    pub rabbitscope: &'static str,
}
