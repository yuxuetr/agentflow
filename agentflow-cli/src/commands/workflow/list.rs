use anyhow::Result;
use crate::commands::ListType;

pub async fn execute(_list_type: ListType) -> Result<()> {
    println!("List command not yet implemented");
    Ok(())
}