use crate::{kernel::sched, print};

pub fn start() -> Result<(), &'static str> {
    sched::spawn(|| print_letters('A')).map_err(|_| "Failed to spawn task_a")?;
    sched::spawn(|| print_letters('B')).map_err(|_| "Failed to spawn task_b")?;

    Ok(())
}

fn print_letters(c: char) {
    for _ in 0..40 {
        print!("{}", c);
        let mut i = 0;
        loop {
            i += 1;
            if i > 2_000_000 {
                break;
            }
            core::hint::spin_loop();
        }
    }
}
