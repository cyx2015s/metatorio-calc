use std::hash::Hash;

use crate::{concept::EntryOpRequest, math::IndexedVec};

impl<T> IndexedVec<T>
where
    T: Clone,
{
    pub fn dnd<F>(&mut self, ui: &mut egui::Ui, id_source: impl Hash, mut f: F)
    where
        F: FnMut(
            &mut egui::Ui,
            usize,
            &mut T,
            egui_dnd::Handle,
            egui_dnd::ItemState,
            &mut EntryOpRequest,
        ),
    {
        let mut delete_target = None;
        let mut clone_target = None;
        let mut virtual_idx = 0;
        egui_dnd::dnd(ui, id_source).show_vec(&mut self.idx, |ui, real_idx, handle, state| {
            let mut op_request = EntryOpRequest::None;
            let item = &mut self.vec[*real_idx];
            f(ui, *real_idx, item, handle, state, &mut op_request);
            if let EntryOpRequest::Drop = op_request {
                delete_target = Some(virtual_idx);
            }
            if let EntryOpRequest::Clone = op_request {
                clone_target = Some(virtual_idx);
            }
            virtual_idx += 1;
        });
        if let Some(idx) = clone_target {
            let value = self[idx].clone();
            self.insert(idx, value);
        }
        if let Some(idx) = delete_target {
            self.remove(idx);
        }
    }
}
