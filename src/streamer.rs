use std::io::{Read, Write, ErrorKind};
use std::net::TcpStream;
use std::process::{ChildStdout, ChildStdin};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{Sender, Receiver, TryRecvError, channel};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::exec;
use crate::siapi;
use siapi::IntApi;
use exec::Exec;

const STREAM_TRANSFER_DELAY: Duration = Duration::from_millis(10);
const STREAM_BUF_SIZE: usize = 4096;

pub struct Stream {
    pub program_id: u32,
    stdout: Receiver<Vec<u8>>,
    stdin: Option<Sender<Vec<u8>>>,
    stdout_handle: JoinHandle<()>,
}

struct RemoteHandler {
    pub program_id: u32,
    handle: JoinHandle<Stream>
}

fn just_msg(tcp: &mut TcpStream, msg: &str) {
    print!("[STREAMER][MSG] {}", msg);
    match tcp.write_all(msg.as_bytes()) {
        _ => ()
    }
}

fn remote_handler(mut stream: Stream, mut tcp: TcpStream) -> Stream {
    let mut is_stdin = stream.stdin.is_some();
    let mut buf: [u8; STREAM_BUF_SIZE] = [0;STREAM_BUF_SIZE];
    tcp.set_read_timeout(Some(STREAM_TRANSFER_DELAY)).unwrap();
    loop {
        match stream.stdout.try_recv() {
            Ok(data) => {
                match tcp.write_all(&data) {
                    Err(_) => {
                        println!("[STREAMER][REMOTE_H] tcp disconnected, break loop..");
                        break;
                    },
                    _ => ()
                }
            },
            Err(TryRecvError::Empty) => (),
            _ => {
                println!("[STREAMER][REMOTE_H] stdout disconnected, break loop..");
                break;
            }
        }
        if is_stdin {
            match tcp.read(&mut buf) {
                Ok(len) => {
                    if len == 0 {
                        println!("[STREAMER][REMOTE_H] tcp disconnected, break loop..");
                        break;                        
                    }
                    match stream.stdin.take() {
                        Some(stdin) => {
                            match stdin.send(buf[..len].to_vec()) {
                                Err(_) => {
                                    println!("[STREAMER][REMOTE_H] stdin disconnected, continue without it..");
                                    is_stdin = false;
                                },
                                _ => stream.stdin = Some(stdin)
                            }
                        },
                        None => ()
                    }
                },
                Err(ref e) if e.kind() == ErrorKind::TimedOut => (),
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => (),
                Err(_) => {
                    println!("[STREAMER][REMOTE_H] tcp disconnected, break loop..");
                    break;
                }
            }
        }
    }
    stream
}

fn stdin_handler(rx: Receiver<Vec<u8>>, mut stdin: ChildStdin) {
    loop {
        match rx.recv() {
            Ok(data) => {
                match stdin.write_all(&data) {
                    Err(e) => {
                        println!("[STREAMER][STDIN_H] fail to write data: {}\n\tbreak loop..", e.to_string());
                        break;
                    },
                    _ => ()
                }
            },
            Err(e) => {
                println!("[STREAMER][STDIN_H] fail to receive data: {}\n\tbreak loop..", e.to_string());
                break;
            }
        }
    }
}

fn stdout_handler(tx: Sender<Vec<u8>>, mut stdout: ChildStdout) {
    let mut buf:[u8;STREAM_BUF_SIZE] = [0;STREAM_BUF_SIZE];
    loop {
        match stdout.read(&mut buf) {
            Ok(len) => {
                if len == 0 {
                    println!("[STREAMER][STDOUT_H] fail to read data: zero len\n\tbreak loop..");
                    break;                    
                }
                match tx.send(buf[..len].to_vec()) {
                    Err(e) => {
                        println!("[STREAMER][STDOUT_H] fail to send data: {}\n\tbreak loop..", e.to_string());
                        break;
                    },
                    _ => ()
                }
            },
            Err(e) => {
                println!("[STREAMER][STDOUT_H] fail to read data: {}\n\tbreak loop..", e.to_string());
                break;
            }
        }
    }
}

fn extract_stream(exec: &Arc<Mutex<Exec>>) -> Option<Stream> {
    let mut exec_unmux = exec.lock().unwrap();
    match exec_unmux.proc.take() {
        Some(mut child) => {
            let stdout: (Receiver<Vec<u8>>, JoinHandle<()>) = match child.stdout.take() {
                Some(stdout) => {
                    let (tx, rx) = channel();
                    let handle = thread::spawn(|| stdout_handler(tx, stdout));
                    (rx, handle)
                },
                None => return None
            };
            let stdin: Option<Sender<Vec<u8>>> = match child.stdin.take() {
                Some(stdin) => {
                    let (tx, rx) = channel();
                    thread::spawn(|| stdin_handler(rx, stdin));
                    Some(tx)
                },
                None => None
            };
            exec_unmux.proc = Some(child);
            Some(Stream {program_id: exec_unmux.program_id, stdout: stdout.0, stdin: stdin, stdout_handle: stdout.1})
        },
        None => None
    }
}

pub struct Streamer {
    streams: Arc<Mutex<Vec<Stream>>>,
    rhandlers: Arc<Mutex<Vec<RemoteHandler>>>
}

impl Streamer {
    pub fn new() -> Self {
        Self {
            streams: Arc::new(Mutex::new(Vec::new())),
            rhandlers: Arc::new(Mutex::new(Vec::new()))
        }
    }

    pub fn get_and_begin_stream(&mut self, execs: &Vec<Arc<Mutex<Exec>>>, api: Arc<Mutex<IntApi>>, bin_path: &str) {
        let api_res = api.lock().unwrap().get_stream();
        match api_res {
            Ok((answ, mut tcp)) => {
                let stream = match self.find_stream(answ.point_program_id) {
                    Some(stream) => stream,
                    None => {
                        match exec::exec_find(&execs, answ.point_program_id) {
                            Some(ex) => {
                                let is_run = ex.lock().unwrap().is_run();
                                if is_run {
                                    if self.rhandler_exists(answ.point_program_id) {
                                        just_msg(&mut tcp, "EXEC STREAM ALREADY USED\n");
                                    } else {
                                        just_msg(&mut tcp, "INTERNAL: EXEC STREAM BROKEN\n");
                                    }
                                    return;
                                } else {
                                    just_msg(&mut tcp, "STARTING PROGRAM..\n");
                                    match exec::start_program(ex.clone(), api, None, bin_path) {
                                        Err(e) => {
                                            let msg = format!("fail to start program: {}\n", e);
                                            just_msg(&mut tcp, &msg);
                                            return;
                                        },
                                        _ => {
                                            match extract_stream(&ex) {
                                                Some(stream) => stream,
                                                None => {
                                                    just_msg(&mut tcp, "INTERNAL: fail to get stdio from started exec\n");
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            None => {
                                just_msg(&mut tcp, "EXEC NOT FOUND\n");
                                return;
                            }
                        }
                    }
                };
                let pid = stream.program_id;
                let rhandle = thread::spawn(|| remote_handler(stream, tcp));
                self.rhandlers.lock().unwrap().push(RemoteHandler {program_id: pid, handle: rhandle});
            },
            Err(e) => {
                println!("fail get stream from server: {}", siapi::get_rc_name(e))
            }
        }
    }

    pub fn last_words(&mut self, program_id: u32) -> String {
        String::new()
    }

    pub fn update_streams(&mut self, execs: &Vec<Arc<Mutex<Exec>>>) {
        // extarct streams from run execs, and delete if not is run
        for ex in execs {
            let mut ex_unmux = ex.lock().unwrap();
            let id = ex_unmux.program_id;
            if ex_unmux.is_run() {
                if !self.stream_exists(id) && !self.rhandler_exists(id) {
                    let name = ex_unmux.name.clone();
                    drop(ex_unmux);
                    match extract_stream(ex) {
                        Some(s) => {
                            self.streams.lock().unwrap().push(s);
                            println!("[STREAMER] add stream for {}", name);
                        },
                        None => {
                            println!("[STREAMER] fail add stream for {}: exec is run, but stdio is None", name);
                        }
                    }
                }
            } else {
                self.remove_wstream(id);
            }
        }

        // return ended remote handlers streams to waiting streams
        loop {
            match self.move_rh_stream() {
                Some(id) => println!("[STREAMER] move stream from remote_handler to waiting stream for program {}", id),
                None => break
            }
        }

        // check broken streams and delete its, stream broken if stdout_handle was finished
        loop {
            match self.remove_broken_stream() {
                Some(id) => println!("[STREAMER] remove broken stream with pid {}", id),
                None => break
            }
        }
    }

    fn remove_broken_stream(&mut self) -> Option<u32> {
        let mut streams_unmux = self.streams.lock().unwrap();
        for i in 0..streams_unmux.len() {
            if streams_unmux[i].stdout_handle.is_finished() {
                let id = streams_unmux[i].program_id;
                streams_unmux.remove(i);
                return Some(id);
            }
        }
        None
    }

    fn move_rh_stream(&mut self) -> Option<u32> {
        let mut streams_unmux = self.streams.lock().unwrap();
        let mut rhandlers_unmux = self.rhandlers.lock().unwrap();
        for i in 0..rhandlers_unmux.len() {
            if rhandlers_unmux[i].handle.is_finished() {
                let rh = rhandlers_unmux.remove(i);
                let stream = rh.handle.join().unwrap();
                let id = stream.program_id;
                streams_unmux.push(stream);
                return Some(id);
            }
        }
        None
    }

    fn remove_wstream(&mut self, program_id: u32) {
        let mut streams_unmux = self.streams.lock().unwrap();
        for i in 0..streams_unmux.len() {
            if streams_unmux[i].program_id == program_id {
                streams_unmux.remove(i);
                println!("[STREAMER] stream with pid {} removed", program_id);
                return;
            }
        }
    }

    fn rhandler_exists(&self, program_id: u32) -> bool {
        let rhandlers_unmux = self.rhandlers.lock().unwrap();
        for rh in &*rhandlers_unmux {
            if rh.program_id == program_id {
                return true;
            }
        }

        false
    }

    fn stream_exists(&self, program_id: u32) -> bool {
        let streams_unmux = self.streams.lock().unwrap();
        for i in 0..streams_unmux.len() {
            if streams_unmux[i].program_id == program_id {
                return true;
            }
        }
        false
    }

    fn find_stream(&mut self, program_id: u32) -> Option<Stream> {
        let mut streams_unmux = self.streams.lock().unwrap();
        for i in 0..streams_unmux.len() {
            if streams_unmux[i].program_id == program_id {
                return Some(streams_unmux.remove(i));
            }
        }
        None
    }
}