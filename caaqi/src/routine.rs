pub trait Routine {
    fn run(&mut self);
}

impl<F: FnMut()> Routine for F {
    fn run(&mut self) {
        self();
    }
}
