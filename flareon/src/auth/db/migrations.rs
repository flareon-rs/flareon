mod m_0001_initial;

pub const MIGRATIONS: &[&dyn ::flareon::db::migrations::DynMigration] =
    &[&m_0001_initial::Migration];
