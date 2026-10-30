pub mod arithmetic {
    mod adc;
    mod cmp;
    mod sbc;
}

pub mod branches {
    mod branch;
}

pub mod inc_dec {
    mod dec;
    mod inc;
}

pub mod jump_call {
    mod jmp;
}

pub mod load_store {
    mod lda;
}

pub mod logical {
    mod and;
}

pub mod shifts {
    mod asl;
    mod lsr;
    mod rol;
    mod ror;
}

pub mod stack_ops {
    mod pha;
}

pub mod system_functions {
    mod brk;
}

pub mod op_codes;
