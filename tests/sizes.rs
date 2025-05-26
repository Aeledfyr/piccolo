use std::mem;

use piccolo::{opcode::OpCode, Callback, Closure, String, Table, Thread, UserData, Value};

#[test]
fn test_sizes() {
    assert!(mem::size_of::<OpCode>() <= 4);

    let ptr_size = mem::size_of::<*const ()>();
    assert_eq!(mem::size_of::<String>(), ptr_size);
    assert_eq!(mem::size_of::<Table>(), ptr_size);
    assert_eq!(mem::size_of::<Closure>(), ptr_size);
    assert_eq!(mem::size_of::<Callback>(), ptr_size);
    assert_eq!(mem::size_of::<Thread>(), ptr_size);
    assert_eq!(mem::size_of::<UserData>(), ptr_size);

    if ptr_size < 8 {
        assert!(mem::size_of::<Value>() <= size_of::<u64>() + ptr_size);
    } else {
        assert!(mem::size_of::<Value>() <= ptr_size * 2);
    }
}
