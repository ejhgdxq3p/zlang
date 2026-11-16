// 临时复现：ureq 3.x 连接本地 node 服务的真实行为
fn main() {
    let 端口: u16 = std::env::args().nth(1).unwrap_or("15555".into()).parse().unwrap();
    let 地址 = format!("http://127.0.0.1:{}/", 端口);
    eprintln!("probe url = {}", 地址);
    eprintln!("http_proxy = {:?}", std::env::var("http_proxy"));
    eprintln!("HTTPS_PROXY = {:?}", std::env::var("HTTPS_PROXY"));
    match ureq::get(&地址).call() {
        Ok(mut r) => {
            eprintln!("OK status = {}", r.status());
            let _ = r.body_mut().read_to_string().unwrap_or_default();
        }
        Err(e) => {
            eprintln!("ERR = {:?}", e);
        }
    }
}
