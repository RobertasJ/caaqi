use smallvec::SmallVec;

pub trait Action {
    type Output;

    fn run(&mut self, cx: &mut ActionContext) -> Result<Self::Output, SelfAdjust>;
}

impl<F: FnMut(&mut ActionContext) -> Result<T, SelfAdjust>, T> Action for F {
    type Output = T;

    fn run(&mut self, cx: &mut ActionContext) -> Result<Self::Output, SelfAdjust> {
        self(cx)
    }
}

#[derive(Debug)]
pub struct SelfAdjust;

pub struct ActionContext {
    subscribers: SmallVec<[(); 1]>,
}

impl ActionContext {
    pub fn new() -> Self {
        Self {
            subscribers: SmallVec::new(),
        }
    }

    pub fn subscribe(&mut self) {
        self.subscribers.push(());
    }
}

pub fn finish_with<T>(val: T) -> Result<T, SelfAdjust> {
    Ok(val)
}

pub fn finish() -> Result<(), SelfAdjust> {
    finish_with(())
}
