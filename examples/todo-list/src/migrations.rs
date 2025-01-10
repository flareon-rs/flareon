mod m_0001_initial;

pub(crate) const MIGRATIONS: &[&dyn ::cot::db::migrations::DynMigration] =
    &[&m_0001_initial::Migration];
