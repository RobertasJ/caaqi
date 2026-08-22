#[derive(Debug, Clone, bon::Builder, Default)]
pub struct SizingNode {
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

#[derive(Debug, Clone, PartialEq)]
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
pub struct PositioningNode {
    x: Option<f32>,
    y: Option<f32>,
}
