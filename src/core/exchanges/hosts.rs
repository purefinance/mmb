#[derive(Clone)]
pub struct Hosts {
    pub web_socket_host: &'static str,
    // Some exchanges have two websockets, for public and private data
    pub web_socket2_host: &'static str,
    pub rest_host: &'static str,
}
