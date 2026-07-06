macro_rules! stub {
    ($name:ident) => {
        pub mod $name {
            pub fn run() -> i32 {
                eprintln!("not implemented yet");
                1
            }
        }
    };
}
pub mod install;
pub mod uninstall;
pub mod init;
pub mod doctor;
stub!(update);
pub mod config_cmd;
