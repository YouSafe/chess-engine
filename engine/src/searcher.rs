use crate::search::Search;
use crate::search_limits::SearchLimits;
use crate::transposition_table::TranspositionTable;
use chess_core::board::Board;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

enum SearcherMessage {
    NewSearchTask(Board, SearchLimits),
    Quit,
}

pub struct Searcher {
    channel_sender: Sender<SearcherMessage>,
    table: Arc<Mutex<TranspositionTable>>,
    stop: Arc<AtomicBool>,
    main_thread_handle: Option<JoinHandle<()>>,
}

impl Searcher {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();

        let table = Arc::new(Mutex::new(TranspositionTable::new()));
        let stop = Arc::new(AtomicBool::new(false));

        Searcher {
            channel_sender: sender,
            table: table.clone(),
            stop: stop.clone(),
            main_thread_handle: Some(thread::spawn(move || loop {
                let message = receiver.recv().expect("could not receive message");

                match message {
                    SearcherMessage::Quit => {
                        eprintln!("not accepting any more search requests");
                        break;
                    }
                    SearcherMessage::NewSearchTask(board, limits) => {
                        stop.store(false, Ordering::SeqCst);
                        let stop_ref = stop.as_ref();
                        let table_ref = &mut table.lock().unwrap();

                        let mut search = Search::new(board, table_ref, stop_ref);

                        let pick = search.find_best_move(limits);
                        println!("bestmove {}", pick.chess_move.unwrap());
                    }
                }
            })),
        }
    }

    pub fn clear_tables(&mut self) {
        self.table.lock().unwrap().clear();
    }

    pub fn initiate_search(&self, board: Board, limits: SearchLimits) {
        self.channel_sender
            .send(SearcherMessage::NewSearchTask(board, limits))
            .expect("could not send new search task");
    }

    pub fn stop_search(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for Searcher {
    fn drop(&mut self) {
        eprintln!("shutting down searcher thread");
        self.stop_search();
        self.channel_sender
            .send(SearcherMessage::Quit)
            .expect("could not send quit message");
        if let Some(handle) = self.main_thread_handle.take() {
            handle.join().expect("could not join main search thread");
        }
    }
}
