#[macro_export]
macro_rules! to_store_err {
    ($expr:expr) => {
        $expr.map_err(|e| {
            wacore::store::error::StoreError::Database(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            )
        })
    };
    ($expr:expr, exec) => {
        $expr.map(|_| ()).map_err(|e| {
            wacore::store::error::StoreError::Database(
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            )
        })
    };
}
