use crate::spline::CurveType;
use crate::state::{AppState, EditMode};

pub fn build_ui(ctx: &egui::Context, state: &mut AppState) {
    // ---- Controls window -----------------------------------------------------
    egui::Window::new("Controls")
        .default_pos([10.0, 10.0])
        .default_size([200.0, 300.0])
        .show(ctx, |ui| {
            match &state.mode {
                EditMode::Idle => {
                    if ui.button("New Line").clicked() {
                        state.new_line();
                    }
                }
                EditMode::Editing { .. } => {
                    if ui.button("Done Editing").clicked() {
                        state.stop_edit();
                    }
                }
            }

            ui.separator();

            let editing_index = match &state.mode {
                EditMode::Editing { spline_index, .. } => Some(*spline_index),
                EditMode::Idle => None,
            };

            // Curve type selector for the spline being edited.
            if let Some(idx) = editing_index {
                egui::ComboBox::from_label("Curve")
                    .selected_text(curve_name(state.splines[idx].curve_type))
                    .show_ui(ui, |ui| {
                        for (ct, label) in [
                            (CurveType::CatmullRom, "Catmull-Rom"),
                            (CurveType::CatmullRomCentripetal, "CR Centripetal"),
                            (CurveType::BSplineInterp, "B-Spline Interp"),
                        ] {
                            if ui
                                .selectable_label(state.splines[idx].curve_type == ct, label)
                                .clicked()
                            {
                                state.splines[idx].curve_type = ct;
                                state.splines[idx].dirty = true;
                            }
                        }
                    });
                ui.separator();
            }

            ui.label("Lines:");

            let mut select_index: Option<usize> = None;
            for (i, spline) in state.splines.iter().enumerate() {
                let label = if editing_index == Some(i) {
                    format!("[EDIT] {}", spline.name)
                } else {
                    spline.name.clone()
                };

                if editing_index.is_none() {
                    if ui.selectable_label(false, &label).clicked() {
                        select_index = Some(i);
                    }
                } else {
                    ui.label(&label);
                }
            }

            if let Some(i) = select_index {
                state.start_edit(i);
            }
        });

    // ---- Right-click context menu --------------------------------------------
    // Latch screen position on the first frame the menu opens (open_context_menu flag).
    if state.open_context_menu {
        let ws = state.window_size;
        let ndc = state.mouse_ndc;
        let px = (ndc[0] + 1.0) / 2.0 * ws[0];
        let py = (1.0 - ndc[1]) / 2.0 * ws[1];
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("ctx_menu_pos"), egui::pos2(px, py));
        });
        state.open_context_menu = false;
    }

    if let Some((sidx, pidx)) = state.context_menu {
        let pos = ctx
            .data(|d| d.get_temp::<egui::Pos2>(egui::Id::new("ctx_menu_pos")))
            .unwrap_or(egui::Pos2::ZERO);

        egui::Area::new(egui::Id::new("ctx_menu"))
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(format!("Point {pidx}")).weak());
                ui.separator();
                if ui.button("Delete Point").clicked() {
                    state.delete_point(sidx, pidx);
                }
            });
    }
}

fn curve_name(ct: CurveType) -> &'static str {
    match ct {
        CurveType::CatmullRom => "Catmull-Rom",
        CurveType::CatmullRomCentripetal => "CR Centripetal",
        CurveType::BSplineInterp => "B-Spline Interp",
    }
}
