use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use guinea_widgets::table::{ColumnSpec, Width, table};
use guinea_core::{UiDispatcher, UiTask, invoke_on_ui, set_ui_dispatcher};
use windows_reactor::{App, Backdrop, Element, RenderCx, SetState, UiMarshaller, WinUIDispatcher, text_block};

struct Row {
    id: u32,
    name: String,
    cpu: f32,
    memory: u64,
}

#[derive(Default)]
struct BenchStats {
    samples: Vec<(f64, f64, f64, u64, u64, u64)>,
}

thread_local! {
    static REQUEST_RERENDER: RefCell<Option<SetState<()>>> = const { RefCell::new(None) };
    static WIDTH: RefCell<Option<Width>> = const { RefCell::new(None) };
}

struct ReactorDispatcher(UiMarshaller);
impl UiDispatcher for ReactorDispatcher {
    fn init(&self) {}
    fn dispatch(&self, task: UiTask) {
        self.0.dispatch(task);
    }
}

fn main() -> anyhow::Result<()> {
    App::new()
        .title("table resize bench")
        .inner_size(1000.0, 700.0)
        .backdrop(Backdrop::Mica)
        .render(bench_root)
        .map_err(|e| anyhow::anyhow!("app failed: {e:?}"))
}

fn bench_root(cx: &mut RenderCx) -> Element {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let dispatcher = WinUIDispatcher::for_current_thread().expect("UI thread dispatcher");
        set_ui_dispatcher(ReactorDispatcher(dispatcher.marshaller()));
    });

    let width = cx.use_ref(Width::fixed(120));
    let width = width.borrow().clone();

    let (_, request_rerender) = cx.use_state(());
    let stats = cx.use_ref(Arc::new(Mutex::new(BenchStats::default())));
    let stats = stats.borrow().clone();

    let initialized = cx.use_ref(false);
    if !*initialized.borrow() {
        *initialized.borrow_mut() = true;

        REQUEST_RERENDER.with(|r| *r.borrow_mut() = Some(request_rerender.clone()));
        WIDTH.with(|w| *w.borrow_mut() = Some(width.clone()));

        // Install render_complete
        let stats_for_render = stats.clone();
        windows_reactor::with_active_host(|host| {
            host.set_render_complete(move |info| {
                stats_for_render.lock().unwrap().samples.push((
                    info.tree_build_ms,
                    info.reconcile_ms,
                    info.effects_ms,
                    info.elements_diffed,
                    info.elements_skipped,
                    info.elements_created,
                ));
            });
        });

        // Bench thread
        let stats_for_bench = stats.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(1));
            for i in 0..200 {
                let w = 120 + (i % 50);
                invoke_on_ui(move || {
                    WIDTH.with(|width| {
                        if let Some(width) = width.borrow().as_ref() {
                            width.set(w);
                        }
                    });
                    REQUEST_RERENDER.with(|r| {
                        if let Some(request) = r.borrow().as_ref() {
                            request.call(());
                        }
                    });
                });
                std::thread::sleep(Duration::from_millis(16));
            }
            std::thread::sleep(Duration::from_secs(1));
            let s = stats_for_bench.lock().unwrap();
            let count = s.samples.len();
            if count > 0 {
                let avg = |idx: usize| {
                    s.samples
                        .iter()
                        .map(|x| match idx {
                            0 => x.0,
                            1 => x.1,
                            2 => x.2,
                            3 => x.3 as f64,
                            4 => x.4 as f64,
                            5 => x.5 as f64,
                            _ => 0.0,
                        })
                        .sum::<f64>()
                        / count as f64
                };
                println!("samples: {}", count);
                println!("avg tree_build: {:.2} ms", avg(0));
                println!("avg reconcile: {:.2} ms", avg(1));
                println!("avg effects: {:.2} ms", avg(2));
                println!("avg elements_diffed: {:.0}", avg(3));
                println!("avg elements_skipped: {:.0}", avg(4));
                println!("avg elements_created: {:.0}", avg(5));
            }
            std::process::exit(0);
        });
    }

    let rows: Vec<Row> = (0..1000)
        .map(|i| Row {
            id: i,
            name: format!("process-{}.exe", i),
            cpu: (i % 100) as f32 / 10.0,
            memory: (i as u64) * 1024 * 1024,
        })
        .collect();

    let columns = vec![
        ColumnSpec::new("name", "Name", 280u64, |r: &Row| text_block(r.name.clone()).into()),
        ColumnSpec::new("cpu", "CPU", width.clone(), |r: &Row| {
            text_block(format!("{:.1}%", r.cpu)).into()
        }),
        ColumnSpec::new("memory", "Memory", 140u64, |r: &Row| {
            text_block(format!("{} MiB", r.memory / 1024 / 1024)).into()
        }),
    ];

    table(cx, rows, columns, |r: &Row| r.id.to_string(), None, None)
}
