use crate::context::{NodeId, UiTree};

#[derive(Debug, Clone, Copy, bon::Builder, Default)]
pub struct Sizing {
    #[builder(default, with = |value: Size| NodeValueState::Pending(value))]
    min_width: NodeValueState<f32, Size>,
    #[builder(default, with = |value: Size| NodeValueState::Pending(value))]
    min_height: NodeValueState<f32, Size>,
    #[builder(default, with = |value: Size| NodeValueState::Pending(value))]
    max_width: NodeValueState<f32, Size>,
    #[builder(default, with = |value: Size| NodeValueState::Pending(value))]
    max_height: NodeValueState<f32, Size>,
    #[builder(default, with = |value: Size| NodeValueState::Pending(value))]
    preferred_width: NodeValueState<f32, Size>,
    #[builder(default, with = |value: Size| NodeValueState::Pending(value))]
    preferred_height: NodeValueState<f32, Size>,
    pub chosen_width: Option<f32>,
    pub chosen_height: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Size {
    Pixels(f32),
    ParentPercent(f32),
    WindowPercent(f32),
    Grow,
    #[default]
    Fit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeValueState<T, F> {
    Computed(T),
    Pending(F),
}

impl<T, F: Default> Default for NodeValueState<T, F> {
    fn default() -> Self {
        Self::Pending(F::default())
    }
}

#[derive(Debug, Clone, bon::Builder, Default)]
pub struct Positioning {
    x: Option<f32>,
    y: Option<f32>,
}

impl UiTree {
    pub fn measure_widths(&mut self) {
        struct Measure {
            width: f32,
            height: f32,
        }

        for root in self.roots.clone() {
            self.traverse_bottom_up_sizing_mut::<Measure>(root, |sizing, children_measure_iter| {
                match (sizing.chosen_width, sizing.chosen_height) {
                    (None, None) => {
                        let mut width: f32 = 0.0;
                        let mut height: f32 = 0.0;

                        for ch_measure in children_measure_iter {
                            width = width.max(ch_measure.width);
                            height += ch_measure.height;
                        }

                        Measure { height, width }
                    }
                    (None, Some(height)) => {
                        let mut width: f32 = 0.0;

                        for ch_measure in children_measure_iter {
                            width = width.max(ch_measure.width);
                        }

                        Measure { width, height }
                    }
                    (Some(width), None) => {
                        let mut height: f32 = 0.0;

                        for ch_measure in children_measure_iter {
                            height += ch_measure.height;
                        }

                        Measure { height, width }
                    }
                    (Some(width), Some(height)) => Measure { width, height },
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {}
