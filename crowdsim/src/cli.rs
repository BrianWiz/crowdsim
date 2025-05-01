use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_replicon::prelude::RepliconChannels;
use bevy_replicon_renet::RenetChannelsExt;
use bevy_replicon_renet::netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
    ServerConfig,
};
use bevy_replicon_renet::renet::{ConnectionConfig, RenetClient, RenetServer};
use clap::Parser;

use crate::server::server_spawn_people;
use crate::{IsClient, IsServer};

#[derive(Parser, Resource)]
enum Cli {
    Server {
        #[arg(short, long, default_value_t = 5000)]
        port: u16,
    },
    Client {
        #[arg(short, long, default_value_t = Ipv4Addr::LOCALHOST.into())]
        ip: IpAddr,

        #[arg(short, long, default_value_t = 5000)]
        port: u16,
    },
}

impl Default for Cli {
    fn default() -> Self {
        Self::parse()
    }
}

fn read_cli_system(
    mut commands: Commands,
    cli: Res<Cli>,
    channels: Res<RepliconChannels>,
) -> Result<(), Box<dyn Error>> {
    const PROTOCOL_ID: u64 = 0;

    match *cli {
        Cli::Server { port } => {
            info!("starting server at port {port}");
            // Get default channel configs
            let server_channels_config = channels.server_configs();
            let client_channels_config = channels.client_configs();

            // Configure replication channels to be reliable with large buffers
            // server_channels_config[1].send_type = bevy_replicon_renet::renet::SendType::ReliableOrdered {
            //     resend_time: std::time::Duration::from_secs_f32(fixed_time.delta_secs())
            // };

            let server = RenetServer::new(ConnectionConfig {
                server_channels_config,
                client_channels_config,
                available_bytes_per_tick: 4 * 1024 * 1024,
                ..Default::default()
            });

            let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
            let server_config = ServerConfig {
                current_time,
                max_clients: 10,
                protocol_id: PROTOCOL_ID,
                authentication: ServerAuthentication::Unsecure,
                public_addresses: Default::default(),
            };
            let transport = NetcodeServerTransport::new(server_config, socket)?;

            commands.insert_resource(server);
            commands.insert_resource(transport);
            commands.insert_resource(IsServer);

            commands.spawn((
                Text::new("Server"),
                TextFont {
                    font_size: 30.0,
                    ..Default::default()
                },
                TextColor::WHITE,
            ));

            server_spawn_people(&mut commands);
        }
        Cli::Client { port, ip } => {
            info!("connecting to {ip}:{port}");
            // Get default channel configs
            let server_channels_config = channels.server_configs();
            let client_channels_config = channels.client_configs();

            // Configure replication channels to be reliable with large buffers
            // server_channels_config[1].send_type = bevy_replicon_renet::renet::SendType::ReliableOrdered {
            //     resend_time: std::time::Duration::from_secs_f32(fixed_time.delta_secs()),
            // };

            let client = RenetClient::new(ConnectionConfig {
                server_channels_config,
                client_channels_config,
                available_bytes_per_tick: 4 * 1024 * 1024,
                ..Default::default()
            });

            let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
            let client_id = current_time.as_millis() as u64;
            let server_addr = SocketAddr::new(ip, port);
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
            let authentication = ClientAuthentication::Unsecure {
                client_id,
                protocol_id: PROTOCOL_ID,
                server_addr,
                user_data: None,
            };
            let transport = NetcodeClientTransport::new(current_time, authentication, socket)?;

            commands.insert_resource(client);
            commands.insert_resource(transport);
            commands.insert_resource(IsClient);

            commands.spawn((
                Text(format!("Client: {client_id}")),
                TextFont {
                    font_size: 30.0,
                    ..default()
                },
                TextColor::WHITE,
            ));
        }
    }

    Ok(())
}

pub struct CliPlugin;
impl Plugin for CliPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cli>();
        app.add_systems(Startup, read_cli_system.map(Result::unwrap));
    }
}
