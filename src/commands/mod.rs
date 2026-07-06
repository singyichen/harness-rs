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
stub!(uninstall);
stub!(init);
stub!(doctor);
pub mod update;
stub!(config_cmd);
