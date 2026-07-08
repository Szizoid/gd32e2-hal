use core::marker::PhantomData;

pub struct Input;
pub struct Output;

pub enum Port {
    A,
    B,
}

pub struct Pin<MODE> {
    port: Port,
    pin: u8,
    _mode: PhantomData<MODE>,
}

const _: () = assert!(core::mem::size_of::<Pin<Output>>() == 2);
