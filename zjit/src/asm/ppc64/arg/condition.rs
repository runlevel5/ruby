/// A condition for a conditional branch to test, which is one bit of CR0 and
/// the value it must have for the branch to be taken.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Condition {
    /// The BO field: 12 branches if the CR bit is set, 4 if it is clear.
    pub bo: u8,

    /// The BI field: the CR bit to test, where bit 0 is LT of CR0.
    pub bi: u8,
}

impl Condition {
    const TRUE: u8 = 0b01100;
    const FALSE: u8 = 0b00100;

    const CR0_LT: u8 = 0;
    const CR0_GT: u8 = 1;
    const CR0_EQ: u8 = 2;
    const CR0_SO: u8 = 3;

    pub const LT: Self = Self { bo: Self::TRUE, bi: Self::CR0_LT };
    pub const GE: Self = Self { bo: Self::FALSE, bi: Self::CR0_LT };
    pub const GT: Self = Self { bo: Self::TRUE, bi: Self::CR0_GT };
    pub const LE: Self = Self { bo: Self::FALSE, bi: Self::CR0_GT };
    pub const EQ: Self = Self { bo: Self::TRUE, bi: Self::CR0_EQ };
    pub const NE: Self = Self { bo: Self::FALSE, bi: Self::CR0_EQ };
    pub const SO: Self = Self { bo: Self::TRUE, bi: Self::CR0_SO };
    pub const NS: Self = Self { bo: Self::FALSE, bi: Self::CR0_SO };

    /// Whether the condition is met when its CR bit is set, rather than when it is clear.
    pub fn expects_set(&self) -> bool {
        self.bo == Self::TRUE
    }

    /// The condition that is met exactly when this one is not
    pub fn inverse(&self) -> Self {
        let bo = if self.expects_set() { Self::FALSE } else { Self::TRUE };
        Self { bo, bi: self.bi }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inverse() {
        assert_eq!(Condition::GE, Condition::LT.inverse());
        assert_eq!(Condition::LE, Condition::GT.inverse());
        assert_eq!(Condition::NE, Condition::EQ.inverse());
        assert_eq!(Condition::NS, Condition::SO.inverse());
        assert_eq!(Condition::SO, Condition::NS.inverse());
    }
}
