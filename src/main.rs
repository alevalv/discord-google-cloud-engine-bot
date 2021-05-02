extern crate google_compute1 as compute1;

use std::{sync::Arc, env};

use serenity::{
  async_trait,
  model::{channel::Message, gateway::Ready},
  prelude::*,

  framework::standard::{
    CommandResult, StandardFramework,
    macros::{command, group},
  },
};

use compute1::api::Instance;
use compute1::Compute;
use yup_oauth2::ServiceAccountKey;
use std::path::Path;

struct ComputeHub;

impl TypeMapKey for ComputeHub {
  type Value = Arc<Compute>;
}

struct ComputeName;

impl TypeMapKey for ComputeName {
  type Value = Arc<String>;
}

#[group]
#[commands(ping, server)]
struct General;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
  async fn ready(&self, _: Context, ready: Ready) {
    println!("{} is connected!", ready.user.name);
  }
}

#[tokio::main]
async fn main() {
  let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
  let machine_name = env::var("MACHINE_NAME")
    .expect("Expected a machine instance name in the environment");
  let gce_json_key_path = env::var("GCE_JSON_KEY_FILE_PATH")
    .expect("Expected a path to the gce key file in the environment");

  let framework = StandardFramework::new()
    .configure(|c| c
      .with_whitespace(true)
      .prefix("~")
    )
    .group(&GENERAL_GROUP);

  let mut client = Client::builder(&token)
    .event_handler(Handler)
    .framework(framework)
    .await
    .expect("Err creating client");

  // This is where we can initially insert the data we desire into the "global" data TypeMap.
  // client.data is wrapped on a RwLock, and since we want to insert to it, we have to open it in
  // write mode, but there's a small thing catch:
  // There can only be a single writer to a given lock open in the entire application, this means
  // you can't open a new write lock until the previous write lock has closed.
  // This is not the case with read locks, read locks can be open indefinitely, BUT as soon as
  // you need to open the lock in write mode, all the read locks must be closed.
  //
  // You can find more information about deadlocks in the Rust Book, ch16-03:
  // https://doc.rust-lang.org/book/ch16-03-shared-state.html
  //
  // All of this means that we have to keep locks open for the least time possible, so we put
  // them inside a block, so they get closed automatically when droped.
  // If we don't do this, we would never be able to open the data lock anywhere else.
  {

    // json account key to access gce.
    let secret: ServiceAccountKey = yup_oauth2::read_service_account_key(
      Path::new(gce_json_key_path.as_str())).await.unwrap();
    let auth = yup_oauth2::ServiceAccountAuthenticator::builder(secret,).build().await.unwrap();

    // Open the data lock in write mode, so keys can be inserted to it.
    let mut data = client.data.write().await;

    // The CommandCounter Value has the following type:
    // Arc<RwLock<HashMap<String, u64>>>
    // So, we have to insert the same type to it.
    data.insert::<ComputeHub>(
      Arc::new(
      Compute::new(
        hyper::Client::builder().build(
          hyper_rustls::HttpsConnector::with_native_roots()),
        auth)));
    data.insert::<ComputeName>(Arc::new(machine_name));
  }

  if let Err(why) = client.start().await {
    eprintln!("Client error: {:?}", why);
  }
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
  msg.reply(ctx, "Pong!").await?;

  Ok(())
}

#[command]
async fn server(ctx: &Context, msg: &Message) -> CommandResult {
  let hub = {
    let data_read = ctx.data.read().await;
    data_read.get::<ComputeHub>().expect("Expected ComputeHub in TypeMap.").clone()
  };
  let machine_name = {
    let data_read = ctx.data.read().await;
    data_read.get::<ComputeName>().expect("Expected ComputeName in TypeMap.").clone()
  };

  if msg.content.to_lowercase().contains("status") {
    let machine_response:Instance = hub.instances().get("trello-rs", "us-east1-d", &machine_name)
      .doit().await.expect("Error accessing google cloud when querying machine").1;
    msg.reply(ctx, format!("Server {} has status {}", machine_response.name.unwrap(), machine_response.status.unwrap())).await?;
  }
  else if msg.content.to_lowercase().contains("start") {
    hub.instances().start("trello-rs", "us-east1-d", &machine_name)
      .doit().await.expect("Error accessing google cloud when querying machine").1;

    msg.reply(ctx, "Sent start command to the server").await?;
  }
  else if msg.content.to_lowercase().contains("stop") {
    hub.instances().stop("trello-rs", "us-east1-d", &machine_name)
      .doit().await.expect("Error accessing google cloud when querying machine").1;

    msg.reply(ctx, "Sent stop command to the server").await?;
  }
  else {
    msg.reply(ctx, "Valid commands are status, start or stop").await?;
  }
  Ok(())
}
