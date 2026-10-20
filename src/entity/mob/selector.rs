//! Java's `GoalSelector` with control flags, for the mobs whose goal tables
//! mix flagged and flagless goals (dolphins and turtles). The animal tables
//! in [`super::goals`] hold only `MOVE` goals, so they keep the simpler
//! one-running-goal selector there.
//!
//! On the selector's full update (`(tickCount + id) % 2 == 0`, or the first
//! two ticks) `GoalSelector.tick` stops running goals that can't continue,
//! frees the flags of stopped goals, then starts, in registration order,
//! every idle goal whose flags are free or held by an interruptible goal of a
//! worse (higher) priority, stopping that holder. Then every running goal
//! ticks. Between full updates only goals that `requiresUpdateEveryTick`
//! tick (`tickRunningGoals(false)`).
//!
//! As in Java, a goal that loses one flag to a new goal stops but keeps any
//! other flag it locked until the next cleanup, and a goal with no flags
//! runs beside everything else.

/// `Goal.Flag.MOVE`.
pub(crate) const MOVE: u8 = 1;
/// `Goal.Flag.LOOK`.
pub(crate) const LOOK: u8 = 2;
/// `Goal.Flag.JUMP`.
pub(crate) const JUMP: u8 = 4;

/// One `goalSelector.addGoal(priority, goal)` and the goal's fixed traits.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Spec {
    pub priority: u8,
    pub flags: u8,
    /// `Goal.isInterruptable`.
    pub interruptable: bool,
    /// `Goal.requiresUpdateEveryTick`.
    pub every_tick: bool,
}

pub(crate) const fn spec(priority: u8, flags: u8) -> Spec {
    Spec { priority, flags, interruptable: true, every_tick: false }
}

/// The goals a mob runs and the flags they hold (never saved, as in Java).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Selector {
    running: u32,
    /// Per flag, the locking goal's index plus one (zero: free).
    locks: [u8; 3],
}

/// A mob's goal implementations, by index into its spec table.
pub(crate) trait Host {
    fn can_use(&mut self, goal: usize) -> bool;
    fn can_continue(&mut self, goal: usize) -> bool;
    fn start(&mut self, goal: usize);
    fn stop(&mut self, goal: usize);
    fn tick(&mut self, goal: usize);
}

fn flag_slots(flags: u8) -> impl Iterator<Item = usize> {
    (0..3).filter(move |f| flags & (1 << f) != 0)
}

impl Selector {
    pub(crate) fn running(&self, goal: usize) -> bool {
        self.running & (1 << goal) != 0
    }

    /// `WrappedGoal.stop`: only a running goal stops.
    fn stop(&mut self, goal: usize, host: &mut impl Host) {
        if self.running(goal) {
            self.running &= !(1 << goal);
            host.stop(goal);
        }
    }

    /// `goalCanBeReplacedForAllFlags`.
    fn replaceable(&self, specs: &[Spec], goal: usize) -> bool {
        flag_slots(specs[goal].flags).all(|f| match self.locks[f] {
            0 => true,
            held => {
                let holder = specs[held as usize - 1];
                holder.interruptable && specs[goal].priority < holder.priority
            }
        })
    }

    /// One tick of the selector; `full` is the selector's full update.
    pub(crate) fn tick(&mut self, specs: &[Spec], full: bool, host: &mut impl Host) {
        debug_assert!(specs.len() <= 32);
        if full {
            for goal in 0..specs.len() {
                if self.running(goal) && !host.can_continue(goal) {
                    self.stop(goal, host);
                }
            }
            for lock in &mut self.locks {
                if *lock != 0 && self.running & (1 << (*lock - 1)) == 0 {
                    *lock = 0;
                }
            }
            for goal in 0..specs.len() {
                if self.running(goal) || !self.replaceable(specs, goal) || !host.can_use(goal) {
                    continue;
                }
                for f in flag_slots(specs[goal].flags) {
                    if self.locks[f] != 0 {
                        self.stop(self.locks[f] as usize - 1, host);
                    }
                    self.locks[f] = goal as u8 + 1;
                }
                self.running |= 1 << goal;
                host.start(goal);
            }
        }
        for (goal, spec) in specs.iter().enumerate() {
            if self.running(goal) && (full || spec.every_tick) {
                host.tick(goal);
            }
        }
    }
}

/// `Goal.reducedTickDelay`: ticks counted in full selector updates.
pub(crate) const fn reduced(ticks: i32) -> i32 {
    (ticks + 1).div_euclid(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host whose goals always want to run and log what happens.
    struct Log {
        usable: u32,
        log: Vec<(char, usize)>,
    }
    impl Host for Log {
        fn can_use(&mut self, goal: usize) -> bool {
            self.usable & (1 << goal) != 0
        }
        fn can_continue(&mut self, goal: usize) -> bool {
            self.usable & (1 << goal) != 0
        }
        fn start(&mut self, goal: usize) {
            self.log.push(('+', goal));
        }
        fn stop(&mut self, goal: usize) {
            self.log.push(('-', goal));
        }
        fn tick(&mut self, goal: usize) {
            self.log.push(('t', goal));
        }
    }

    /// A better goal takes a flag from a worse interruptible one; a goal of
    /// the same priority, or one held by an uninterruptible goal, waits; a
    /// flagless goal runs beside both.
    #[test]
    fn flags_follow_javas_goal_selector() {
        let specs = [
            Spec { priority: 0, flags: MOVE | LOOK, interruptable: false, every_tick: false },
            spec(2, MOVE),
            spec(2, MOVE),
            spec(4, 0),
            spec(5, MOVE | LOOK),
        ];
        let mut s = Selector::default();
        let mut host = Log { usable: 0b11110, log: Vec::new() };
        s.tick(&specs, true, &mut host);
        // Goal 1 takes MOVE; goal 2 (same priority) and goal 4 (worse) wait.
        assert!(s.running(1) && !s.running(2) && s.running(3) && !s.running(4));
        // The breath goal (0) outranks it and can't be interrupted afterwards.
        host.usable = 0b11111;
        host.log.clear();
        s.tick(&specs, true, &mut host);
        assert_eq!(host.log, vec![('-', 1), ('+', 0), ('t', 0), ('t', 3)]);
        host.log.clear();
        s.tick(&specs, false, &mut host);
        assert!(host.log.is_empty(), "only every-tick goals tick between full updates");
        // Once it ends, the next full update gives MOVE to goal 1 again.
        host.usable = 0b11110;
        s.tick(&specs, true, &mut host);
        assert!(!s.running(0) && s.running(1) && s.running(3));
        assert_eq!(reduced(700), 350);
        assert_eq!(reduced(25), 13);
    }
}
