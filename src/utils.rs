
pub struct Utils {

}

impl Utils {
    pub fn opt_u64_to_opt_i64(opt_u64: Option<u64>) -> Option<i64> {
        opt_u64.map(|id| id as i64)
    }
}