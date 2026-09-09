//! AI Workload Monitoring Example
//!
//! This example demonstrates real-time monitoring of AI training and inference workloads,
//! including support for TPU hardware and cloud providers.
//!
//! Features demonstrated:
//! - AI framework detection (PyTorch, TensorFlow, JAX, etc.)
//! - Training metrics (epochs, steps, loss, accuracy)
//! - Inference metrics (throughput, latency percentiles)
//! - Distributed training configuration
//! - TPU configuration detection
//! - Cloud provider identification
//! - GPU usage attribution

use ironmonlib::{AiWorkloadMonitor, GpuCollection, ProcessMonitor};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AI Workload Monitor ===\n");

    // Initialize GPU collection for GPU attribution
    let gpus = GpuCollection::auto_detect().ok();
    let gpu_count = gpus.as_ref().map(|g| g.device_count()).unwrap_or(0);
    println!("Detected {} GPU(s)\n", gpu_count);

    // Initialize AI workload monitor
    let mut ai_monitor = AiWorkloadMonitor::new()?;
    println!("AI workload monitor initialized\n");

    // Also initialize process monitor for correlation
    let mut process_monitor = if let Some(gpus) = gpus {
        ProcessMonitor::with_gpus(gpus)?
    } else {
        ProcessMonitor::without_gpu()?
    };

    println!("Monitoring AI workloads... (Press Ctrl+C to stop)\n");
    println!("{:=<120}", "");

    loop {
        // Detect AI workloads
        let workloads = ai_monitor.detect_workloads()?;

        if workloads.is_empty() {
            println!("\rNo AI workloads detected. Scanning...{}", " ".repeat(50));
        } else {
            print!("\x1B[2J\x1B[1;1H"); // Clear screen
            println!("=== AI Workload Monitor ===");
            println!("Found {} active AI workload(s)\n", workloads.len());

            for (idx, workload) in workloads.iter().enumerate() {
                println!(
                    "─── Workload {} ───────────────────────────────────────────────────",
                    idx + 1
                );
                println!("  PID:        {}", workload.pid);
                println!("  Name:       {}", workload.name);
                println!("  Framework:  {}", workload.framework);
                println!("  Type:       {}", workload.workload_type);
                println!("  Cloud:      {:?}", workload.cloud_provider);

                // Show GPU usage
                if !workload.gpu_indices.is_empty() {
                    println!("  GPUs:       {:?}", workload.gpu_indices);
                } else {
                    println!("  GPUs:       Not using GPU");
                }

                // Show TPU configuration if applicable
                if let Some(tpu) = &workload.tpu_config {
                    println!("\n  TPU Configuration:");
                    // Each of these is absent when the environment did not
                    // declare it. Printing "not reported" rather than a default
                    // keeps the distinction the type now carries.
                    println!(
                        "    Type:     {}",
                        tpu.tpu_type.as_deref().unwrap_or("not reported")
                    );
                    match tpu.num_cores {
                        Some(cores) => println!("    Cores:    {}", cores),
                        None => println!("    Cores:    not reported"),
                    }
                    println!(
                        "    Topology: {}",
                        tpu.topology.as_deref().unwrap_or("not reported")
                    );
                    if let Some(zone) = &tpu.zone {
                        println!("    Zone:     {}", zone);
                    }
                }

                // Show distributed training configuration
                if let Some(dist) = &workload.distributed_config {
                    println!("\n  Distributed Training:");
                    println!("    World Size:  {}", dist.world_size);
                    println!("    Rank:        {}", dist.rank);
                    println!("    Local Rank:  {}", dist.local_rank);
                    println!("    Backend:     {}", dist.backend);
                    if let Some(addr) = &dist.master_addr {
                        println!(
                            "    Master:      {}:{}",
                            addr,
                            dist.master_port
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "?".to_string())
                        );
                    }
                }

                // Show training metrics
                if let Some(training) = &workload.training_metrics {
                    println!("\n  Training Metrics:");
                    println!(
                        "    Epoch:       {}/{}",
                        training.current_epoch, training.total_epochs
                    );
                    println!(
                        "    Step:        {}/{}",
                        training.current_step, training.steps_per_epoch
                    );
                    println!("    Loss:        {:.6}", training.current_loss);

                    if let Some(val_loss) = training.validation_loss {
                        println!("    Val Loss:    {:.6}", val_loss);
                    }

                    if let Some(acc) = training.training_accuracy {
                        println!("    Accuracy:    {:.2}%", acc * 100.0);
                    }

                    if let Some(lr) = training.learning_rate {
                        println!("    Learn Rate:  {:.2e}", lr);
                    }

                    if let Some(eta) = training.eta_seconds {
                        let hours = eta / 3600;
                        let minutes = (eta % 3600) / 60;
                        let seconds = eta % 60;
                        println!(
                            "    ETA:         {}h {:02}m {:02}s",
                            hours, minutes, seconds
                        );
                    }

                    // Show progress bar
                    if let Some(progress) = workload.training_progress_percent() {
                        let bar_width = 40;
                        let filled = (progress / 100.0 * bar_width as f32) as usize;
                        let bar = "█".repeat(filled) + &"░".repeat(bar_width - filled);
                        println!("    Progress:    [{}] {:.1}%", bar, progress);
                    }
                }

                // Show inference metrics
                if let Some(inference) = &workload.inference_metrics {
                    println!("\n  Inference Metrics:");
                    println!("    Throughput:  {:.2} samples/sec", inference.throughput);
                    println!("    Batch Size:  {}", inference.batch_size);
                    println!("    Latency:");
                    println!("      Average:   {:.2} ms", inference.latency_avg_ms);
                    println!("      P50:       {:.2} ms", inference.latency_p50_ms);
                    println!("      P95:       {:.2} ms", inference.latency_p95_ms);
                    println!("      P99:       {:.2} ms", inference.latency_p99_ms);
                    println!("    Total:       {} samples", inference.total_samples);

                    if let Some(model) = &inference.model_name {
                        println!("    Model:       {}", model);
                    }
                }

                // Correlate with process monitor
                let processes = process_monitor.processes()?;
                if let Some(proc) = processes.iter().find(|p| p.pid == workload.pid) {
                    println!("\n  Resource Usage:");
                    println!("    CPU:         {:.1}%", proc.cpu_percent);
                    println!("    Memory:      {:.2} GB", proc.memory_mb() / 1024.0);

                    if proc.is_gpu_process() {
                        match proc.gpu_memory_mb() {
                            Some(mb) => println!("    GPU Memory:  {:.2} GB", mb / 1024.0),
                            None => println!(
                                "    GPU Memory:  not reported (NVML does not expose this under WDDM)"
                            ),
                        }

                        // Show per-process engine utilization if available
                        if let Some(gfx) = proc.gfx_engine_used {
                            println!("    GFX Engine:  {} ns", gfx);
                        }
                        if let Some(compute) = proc.compute_engine_used {
                            println!("    Compute:     {} ns", compute);
                        }
                        if let Some(enc) = proc.enc_engine_used {
                            println!("    Encoder:     {} ns", enc);
                        }
                        if let Some(dec) = proc.dec_engine_used {
                            println!("    Decoder:     {} ns", dec);
                        }

                        println!("    GPU Type:    {}", proc.gpu_process_type);
                    }
                }

                // Show relevant environment variables
                if !workload.env_vars.is_empty() {
                    let important_vars = [
                        "CUDA_VISIBLE_DEVICES",
                        "WORLD_SIZE",
                        "RANK",
                        "LOCAL_RANK",
                        "MASTER_ADDR",
                        "MASTER_PORT",
                        "TPU_NAME",
                        "NCCL_DEBUG",
                    ];

                    let relevant: Vec<_> = workload
                        .env_vars
                        .iter()
                        .filter(|(k, _)| important_vars.contains(&k.as_str()))
                        .collect();

                    if !relevant.is_empty() {
                        println!("\n  Environment:");
                        for (key, value) in relevant {
                            println!("    {:<20} = {}", key, value);
                        }
                    }
                }

                println!();
            }

            println!("{:=<120}", "");
        }

        // Update every 5 seconds
        thread::sleep(Duration::from_secs(5));
    }
}
