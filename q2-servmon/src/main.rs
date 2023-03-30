use clap::Parser;
use std::process::{Child, Command};
use std::{process, thread};
use std::borrow::{Cow, BorrowMut};
use std::cell::RefCell;
use std::sync::Mutex;
use std::time::Duration;
use q2_proto::Q2ProtoClient;


#[derive(Parser)]
#[command(author, version, about = "q2-servmon: monitor and auto-reboot quake 2 servers")]
struct Args {
    
    /// port to use and ping status to
    #[arg(short, long, default_value_t = 27910)]
    port: u16,

    /// dedicated server to run
    #[arg(short, long, default_value = "q2proded")]
    executable: String,
    
    /// send a status request every this many seconds
    #[arg(short = 'i', long, default_value_t = 5)]
    status_interval: u16,
    
    /// wait this long for a status response in seconds
    #[arg(short = 't', long, default_value_t = 1)]
    status_timeout: u16,
    
    /// arguments to forward to the executable
    #[arg(last = true)]
    exec_args: Vec<String>
}

static CHILD: Mutex<RefCell<Option<Child>>> = Mutex::new(RefCell::new(None));

fn main() {
    let mut args = Args::parse();

    args.exec_args.append(&mut vec![
        format!("+set port {}", args.port),
        format!("+set net_port {}", args.port) // for q2pro
    ]);

    ctrlc::set_handler(|| {
        try_kill_child();

        process::exit(0);
    }).expect("couldn't set ctrl+c handler");
    
    loop {
        if !run_monitor(&args) {
            break
        }
    }
}

fn try_kill_child() {
    let maybe_proc = CHILD.lock();
    if let Ok(mut maybe_child) = maybe_proc {
        let cell = maybe_child.get_mut();
        if let Some(proc) = cell.borrow_mut() {
            match proc.kill()
            {
                Ok(_) => { println!("process killed") }
                Err(_) => { eprintln!("couldn't kill process, maybe it's already dead") }
            }
        } else {
            eprintln!("tried to kill child process, but no child process was set up");
        }
    } else {
        eprintln!("couldn't get lock on child process");
    }
}

/* true for loop again, false for quit */
fn run_monitor(args: &Args) -> bool {
    println!("launching process (ctrl+c on the monitor kills the child process)");
    
    let mut command = Command::new(&args.executable);
    command.args(&args.exec_args);
    
    println!("full command is: '{} {}'", 
             command.get_program().to_string_lossy(),
             command.get_args()
                 .map(|x| x.to_string_lossy())
                 .collect::<Vec<Cow<'_, str>>>()
                 .join(" "));

    let spawned_child = command.spawn();

    if let Ok(proc) = spawned_child {
        if let Ok(mut s) = CHILD.lock() {
            s.borrow_mut().replace(Some(proc));
        } else {
            eprintln!("couldn't acquire mutex for child process?");
        }

        let addr = format!("127.0.0.1:{}", args.port);
        let client = Q2ProtoClient::new(&addr, "127.0.0.1", 0, "q2-servmon");
        if let Some(cl) = client {
            cl.set_read_timeout(Duration::from_secs(args.status_timeout as u64))
                .expect("couldn't set read timeout on status socket");

            loop {
                thread::sleep(Duration::from_secs(args.status_interval as u64));
                if cl.status().is_none() {
                    try_kill_child();
                    eprintln!("server is down. exiting check loop.");
                    return true
                }
            }
        } else {
            eprintln!("failed to create client");
            return false
        }

    } else if let Err(e) = spawned_child {
        eprintln!("failed to spawn child process for monitoring: {}", e.to_string());
        return false
    }
    
    true
}