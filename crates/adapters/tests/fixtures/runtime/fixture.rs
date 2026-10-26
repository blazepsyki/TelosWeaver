use std::{
    io::{self, Read, Write},
    process::Command,
    time::Duration,
};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--version") => println!("tool 1.2.3"),
        Some("echo") => {
            let mut input = Vec::new();
            io::stdin().read_to_end(&mut input).unwrap();
            io::stdout().write_all(&input).unwrap();
            eprintln!("stderr-tail");
        }
        Some("long") => {
            println!("{}", "x".repeat(20000));
            println!("tail");
        }
        Some("flood") => loop {
            println!("{}", "x".repeat(256));
        },
        Some("sleep") => loop {
            std::thread::sleep(Duration::from_millis(50));
        },
        Some("tree" | "orphan" | "detached-tree" | "detached-orphan") => {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.arg("sleep");
            #[cfg(unix)]
            if args[1].starts_with("detached-") {
                use std::os::unix::process::CommandExt;
                unsafe extern "C" {
                    fn setsid() -> i32;
                }
                unsafe {
                    command.pre_exec(|| {
                        if setsid() < 0 {
                            Err(io::Error::last_os_error())
                        } else {
                            Ok(())
                        }
                    });
                }
            }
            let child = command.spawn().unwrap();
            println!("{}", child.id());
            io::stdout().flush().unwrap();
            if args[1].ends_with("tree") {
                loop {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
        Some("args") => {
            for arg in &args[2..] {
                println!("{arg}");
            }
        }
        Some("env") => println!(
            "{}",
            std::env::var("TELOS_RUNTIME_TEST").unwrap_or_else(|_| "missing".into())
        ),
        _ => std::process::exit(7),
    }
}
