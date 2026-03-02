//! 后台任务队列性能基准测试
//!
//! 对比四种方案在不同负载下的提交延迟和端到端吞吐量：
//! - `tokio::spawn`（最初方案）
//! - `tokio::sync::mpsc`（round-robin 多 worker）
//! - `flume`（MPMC 共享队列，当前方案）
//! - `kanal`（高性能 MPMC）
//!
//! 运行: cargo bench -p app --bench task_queue

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;

type BoxTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// ========== 基准测试 1: 提交开销 ==========
fn bench_submission_overhead(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("1_submission_overhead");

    for n in [100, 1000, 5000] {
        // tokio::spawn
        group.bench_with_input(BenchmarkId::new("tokio_spawn", n), &n, |b, &n| {
            b.to_async(&rt).iter(|| async move {
                for _ in 0..n {
                    tokio::spawn(async {});
                }
            });
        });

        // tokio::sync::mpsc
        group.bench_with_input(BenchmarkId::new("tokio_mpsc", n), &n, |b, &n| {
            b.to_async(&rt).iter(|| async move {
                let (tx, mut rx) = mpsc::channel::<BoxTask>(n);
                tokio::spawn(async move {
                    while let Some(t) = rx.recv().await {
                        t.await;
                    }
                });
                for _ in 0..n {
                    let _ = tx.try_send(Box::pin(async {}));
                }
            });
        });

        // flume MPMC
        group.bench_with_input(BenchmarkId::new("flume_mpmc", n), &n, |b, &n| {
            b.to_async(&rt).iter(|| async move {
                let (tx, rx) = flume::bounded::<BoxTask>(n);
                for _ in 0..4 {
                    let rx = rx.clone();
                    tokio::spawn(async move {
                        while let Ok(t) = rx.recv_async().await {
                            t.await;
                        }
                    });
                }
                for _ in 0..n {
                    let _ = tx.try_send(Box::pin(async {}));
                }
            });
        });

        // kanal MPMC
        group.bench_with_input(BenchmarkId::new("kanal_mpmc", n), &n, |b, &n| {
            b.to_async(&rt).iter(|| async move {
                let (tx, rx) = kanal::bounded_async::<BoxTask>(n);
                for _ in 0..4 {
                    let rx = rx.clone();
                    tokio::spawn(async move {
                        while let Ok(t) = rx.recv().await {
                            t.await;
                        }
                    });
                }
                for _ in 0..n {
                    let _ = tx.try_send(Box::pin(async {}));
                }
            });
        });
    }

    group.finish();
}

/// ========== 基准测试 2: 端到端（4 workers，模拟 1ms IO）==========
fn bench_end_to_end(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("2_end_to_end_4workers");
    group.sample_size(20);

    for n in [50, 200] {
        // tokio::spawn: 全部并发
        group.bench_with_input(
            BenchmarkId::new("tokio_spawn", n),
            &n,
            |b, &n| {
                b.to_async(&rt).iter(|| async move {
                    let mut handles = Vec::with_capacity(n);
                    for _ in 0..n {
                        handles.push(tokio::spawn(async {
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        }));
                    }
                    for h in handles {
                        let _ = h.await;
                    }
                });
            },
        );

        // tokio::mpsc round-robin 4 workers
        group.bench_with_input(
            BenchmarkId::new("tokio_mpsc_rr4", n),
            &n,
            |b, &n| {
                b.to_async(&rt).iter(|| async move {
                    let mut txs = Vec::new();
                    let mut consumers = Vec::new();
                    for _ in 0..4 {
                        let (tx, mut rx) = mpsc::channel::<BoxTask>(n);
                        txs.push(tx);
                        consumers.push(tokio::spawn(async move {
                            while let Some(t) = rx.recv().await {
                                t.await;
                            }
                        }));
                    }
                    for i in 0..n {
                        let _ = txs[i % 4].try_send(Box::pin(async {
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        }));
                    }
                    drop(txs);
                    for c in consumers {
                        let _ = c.await;
                    }
                });
            },
        );

        // flume MPMC 4 workers
        group.bench_with_input(
            BenchmarkId::new("flume_mpmc_4w", n),
            &n,
            |b, &n| {
                b.to_async(&rt).iter(|| async move {
                    let (tx, rx) = flume::bounded::<BoxTask>(n);
                    let mut consumers = Vec::new();
                    for _ in 0..4 {
                        let rx = rx.clone();
                        consumers.push(tokio::spawn(async move {
                            while let Ok(t) = rx.recv_async().await {
                                t.await;
                            }
                        }));
                    }
                    for _ in 0..n {
                        let _ = tx.try_send(Box::pin(async {
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        }));
                    }
                    drop(tx);
                    for c in consumers {
                        let _ = c.await;
                    }
                });
            },
        );

        // kanal MPMC 4 workers
        group.bench_with_input(
            BenchmarkId::new("kanal_mpmc_4w", n),
            &n,
            |b, &n| {
                b.to_async(&rt).iter(|| async move {
                    let (tx, rx) = kanal::bounded_async::<BoxTask>(n);
                    let mut consumers = Vec::new();
                    for _ in 0..4 {
                        let rx = rx.clone();
                        consumers.push(tokio::spawn(async move {
                            while let Ok(t) = rx.recv().await {
                                t.await;
                            }
                        }));
                    }
                    for _ in 0..n {
                        let _ = tx.try_send(Box::pin(async {}));
                    }
                    drop(tx);
                    for c in consumers {
                        let _ = c.await;
                    }
                });
            },
        );
    }

    group.finish();
}

/// ========== 基准测试 3: 突发 10k 任务 ==========
fn bench_burst(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("3_burst_10k");
    group.sample_size(20);

    let n = 10_000;

    group.bench_function("tokio_spawn", |b| {
        b.to_async(&rt).iter(|| async move {
            for _ in 0..n {
                tokio::spawn(async {});
            }
            tokio::task::yield_now().await;
        });
    });

    group.bench_function("tokio_mpsc_4096", |b| {
        b.to_async(&rt).iter(|| async move {
            let (tx, mut rx) = mpsc::channel::<BoxTask>(4096);
            tokio::spawn(async move {
                while let Some(t) = rx.recv().await {
                    t.await;
                }
            });
            for _ in 0..n {
                let _ = tx.try_send(Box::pin(async {}));
            }
        });
    });

    group.bench_function("flume_mpmc_4096", |b| {
        b.to_async(&rt).iter(|| async move {
            let (tx, rx) = flume::bounded::<BoxTask>(4096);
            for _ in 0..4 {
                let rx = rx.clone();
                tokio::spawn(async move {
                    while let Ok(t) = rx.recv_async().await {
                        t.await;
                    }
                });
            }
            for _ in 0..n {
                let _ = tx.try_send(Box::pin(async {}));
            }
        });
    });

    group.bench_function("kanal_mpmc_4096", |b| {
        b.to_async(&rt).iter(|| async move {
            let (tx, rx) = kanal::bounded_async::<BoxTask>(4096);
            for _ in 0..4 {
                let rx = rx.clone();
                tokio::spawn(async move {
                    while let Ok(t) = rx.recv().await {
                        t.await;
                    }
                });
            }
            for _ in 0..n {
                let _ = tx.try_send(Box::pin(async {}));
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_submission_overhead, bench_end_to_end, bench_burst);
criterion_main!(benches);
