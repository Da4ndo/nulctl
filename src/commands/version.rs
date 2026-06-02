use crate::auth::fetch_agent_version;
use crate::connection::Connection;

pub async fn run(conn: &mut Connection) -> Result<(), String> {
	println!("{}", fetch_agent_version(conn).await);
	Ok(())
}
