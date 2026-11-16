#[path = "库安装器.rs"]     pub mod 库安装器;
#[path = "镜像源客户端.rs"] pub mod 镜像源客户端;
#[path = "库版本解析.rs"]   pub mod 库版本解析;
#[path = "存储打包.rs"]     pub mod 存储打包;
#[path = "校验.rs"]         pub mod 校验;
#[path = "凭据.rs"]         pub mod 凭据;

pub use 镜像源客户端 as 镜像源;