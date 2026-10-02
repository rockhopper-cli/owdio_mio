// src/ui/setup_model.rs
use eframe::egui;

/// Helper to render a copyable monospace command box
fn command_box(ui: &mut egui::Ui, code: &str) {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().code_bg_color)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(code).monospace())
                        .wrap()
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("📋 Copy").clicked() {
                        ui.ctx().copy_text(code.to_string());
                    }
                });
            });
        });
    ui.add_space(8.0);
}

pub fn show(ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Local Model Setup (whisper.cpp with NVIDIA GPU)");
        ui.add_space(10.0);

        // --- Step 1 ---
        ui.label(egui::RichText::new("1. NVIDIA Container Toolkit (openSUSE)").strong());
        ui.label(
            "If you have a relatively new NVIDIA graphics card that works with the G07 version \
            of the openSUSE NVIDIA drivers, install and setup the container toolkit:"
        );
        ui.add_space(4.0);
        command_box(ui, "sudo zypper in nvidia-container-toolkit");

        ui.add_space(4.0);
        command_box(ui, "sudo nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml");


        // --- Step 2 ---
        ui.label(egui::RichText::new("2. Model Directory").strong());
        ui.label("Create a dedicated folders for your llm and whisper models:");
        ui.add_space(4.0);
        command_box(ui, "mkdir -p ~/podman_containers/ai_models/llm ~/podman_containers/ai_models/whisper");

        // --- Step 3 ---
        ui.label(egui::RichText::new("3. Download Model").strong());
        ui.label(
            "Move to the directory and download the relevant model. The one below is \
            Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf (optimized for cards with <8GB VRAM):"
        );
        ui.add_space(4.0);
        command_box(
            ui,
            "cd ~/podman_containers/ai_models/llm\ncurl -L -O \"https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/qwen2.5-coder-7b-instruct-q4_k_m.gguf\"",
        );

        ui.label("Standardize filename:");
        ui.add_space(4.0);
        command_box(ui, "mv qwen2.5-coder-7b-instruct-q4_k_m.gguf Qwen2.5-Coder-7B-Instruct-Q4_K_M.ggufr");

        ui.label(
            "Move to the directory and download the relevant model. The one below is \
            large-v3-turbo-q5_0 (optimized for cards with <8GB VRAM):"
        );
        ui.add_space(4.0);
        command_box(
            ui,
            "cd ~/podman_containers/ai_models/whisper\ncurl -L -O \"https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin\"",
        );


        // --- Step 4 ---
        ui.label(egui::RichText::new("4. Run LLM Server").strong());
        ui.label(
            "qwen2.5-coder-7b-instruct is the llm designed for capable of summarizing transcripts and extracting meeting notes.\n\
            Run the container on port 8080:"
        );
        ui.add_space(4.0);

        let podman_llmcmd = r#"podman run -d \
  --name qwen-coder \
  --restart unless-stopped \
  --device nvidia.com/gpu=all \
  -p 127.0.0.1:8080:8080 \
  -v ~/podman_containers/ai_models/llm:/models:Z \
  ghcr.io/ggml-org/llama.cpp:server-cuda \
  -m /models/qwen2.5-coder-7b-instruct-q4_k_m.gguf \
  --host 0.0.0.0 \
  --port 8080 \
  -ngl 24 \
  -c 4096"#;

        command_box(ui, podman_llmcmd);


        // --- Step 5 ---
        ui.label(egui::RichText::new("5. Run Speech-to-Text Server").strong());
        ui.label(
            "whisper.cpp ships with a web server (whisper-server) designed for transcription endpoints.\n\
            Run the container on port 8081:"
        );
        ui.add_space(4.0);

        let podman_cmd = r#"podman run -d \
  --name whisper-server \
  --restart unless-stopped \
  --device nvidia.com/gpu=all \
  --security-opt label=disable \
  -p 127.0.0.1:8081:8081 \
  -v ~/podman_containers/ai_models/whisper:/models \
  --entrypoint /app/build/bin/whisper-server \
  ghcr.io/ggml-org/whisper.cpp:main-cuda \
  --host 0.0.0.0 \
  --port 8081 \
  -m /models/ggml-large-v3-turbo-q5_0.bin \
  --max-context 0 \
  --convert"#;

        command_box(ui, podman_cmd);
    });
}