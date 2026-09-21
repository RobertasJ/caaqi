pub trait Action {
    type Output;

    fn run(&mut self) -> Result<Self::Output, SelfAdjust>;
}

impl<F: FnMut() -> Result<T, SelfAdjust>, T> Action for F {
    type Output = T;

    fn run(&mut self) -> Result<Self::Output, SelfAdjust> {
        self()
    }
}

#[derive(Debug)]
pub struct SelfAdjust;

pub fn finish_with<T>(val: T) -> Result<T, SelfAdjust> {
    Ok(val)
}

pub fn finish() -> Result<(), SelfAdjust> {
    finish_with(())
}
