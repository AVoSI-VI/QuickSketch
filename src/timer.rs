//! Countdown that stops at zero. Drawing stays available the whole time.

use std::time::Instant;

use egui::{ComboBox, DragValue, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerPreset {
    Sec30,
    Min5,
    Min20,
    Custom,
}

pub struct SketchTimer {
    preset: TimerPreset,
    custom_secs: u32,
    deadline: Option<Instant>,
    /// Seconds left after the countdown has stopped at zero.
    stopped_at_zero: bool,
}

impl Default for SketchTimer {
    fn default() -> Self {
        Self {
            preset: TimerPreset::Min5,
            custom_secs: 60,
            deadline: None,
            stopped_at_zero: false,
        }
    }
}

impl SketchTimer {
    pub fn show(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Timer");
            ComboBox::from_id_salt("timer-preset")
                .selected_text(self.preset_label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.preset, TimerPreset::Sec30, "30 seconds");
                    ui.selectable_value(&mut self.preset, TimerPreset::Min5, "5 minutes");
                    ui.selectable_value(&mut self.preset, TimerPreset::Min20, "20 minutes");
                    ui.selectable_value(&mut self.preset, TimerPreset::Custom, "Custom");
                });
            if self.preset == TimerPreset::Custom {
                ui.add(
                    DragValue::new(&mut self.custom_secs)
                        .range(1..=86_400)
                        .suffix(" s"),
                );
            }
            if ui.button("Start").clicked() {
                self.deadline = Some(Instant::now() + self.duration());
                self.stopped_at_zero = false;
            }
            ui.monospace(format_secs(self.display_secs()));
        });
    }

    /// Returns true while the countdown is still running.
    pub fn tick(&mut self) -> bool {
        let Some(deadline) = self.deadline else {
            return false;
        };
        if Instant::now() >= deadline {
            self.deadline = None;
            self.stopped_at_zero = true;
            return false;
        }
        true
    }

    fn duration(&self) -> std::time::Duration {
        let secs = match self.preset {
            TimerPreset::Sec30 => 30,
            TimerPreset::Min5 => 5 * 60,
            TimerPreset::Min20 => 20 * 60,
            TimerPreset::Custom => self.custom_secs.max(1),
        };
        std::time::Duration::from_secs(u64::from(secs))
    }

    fn display_secs(&self) -> f32 {
        if let Some(deadline) = self.deadline {
            return deadline
                .saturating_duration_since(Instant::now())
                .as_secs_f32();
        }
        if self.stopped_at_zero {
            return 0.0;
        }
        self.duration().as_secs_f32()
    }

    fn preset_label(&self) -> &'static str {
        match self.preset {
            TimerPreset::Sec30 => "30 seconds",
            TimerPreset::Min5 => "5 minutes",
            TimerPreset::Min20 => "20 minutes",
            TimerPreset::Custom => "Custom",
        }
    }
}

fn format_secs(secs: f32) -> String {
    let total = secs.ceil() as u32;
    format!("{}:{:02}", total / 60, total % 60)
}
