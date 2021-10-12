// Copyright 2020 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fs;
use std::fs::File;
use std::io;
use std::path::Path;

use clap::ArgMatches;
use flate2::read::GzDecoder;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use tar::Archive;

use crate::cmds::Config;
use crate::cmds::SwitchCommand;
use crate::cmds::Writer;
use crate::error::Result;

#[derive(Clone)]
pub struct FetchCommand {
    conf: Config,
}

impl FetchCommand {
    pub fn create(conf: Config) -> Self {
        FetchCommand { conf }
    }

    //(TODO(zhihanz)) general get_architecture similar to install-databend.sh
    fn get_architecture(&self) -> Result<String> {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let mut clib = "gnu";

        // Check musl
        let (_, musl, _) = run_script::run_script!(r#"ldd --version 2>&1 | grep -q 'musl'"#)?;
        if !musl.is_empty() {
            clib = "musl";
        }

        // Check rosetta
        let (_, rosetta, _) = run_script::run_script!(r#"uname -a"#)?;
        if rosetta.contains("Darwin") && rosetta.contains("arm64") {
            return Ok("aarch64-apple-darwin".to_string());
        }
        let os = match os {
            "darwin" => "apple-darwin".to_string(),
            "macos" => "apple-darwin".to_string(),
            "linux" => format!("unknown-linux-{}", clib),
            _ => os.to_string(),
        };

        Ok(format!("{}-{}", arch, os))
    }

    fn get_latest_tag(&self) -> Result<String> {
        let tag_url = self.conf.tag_url.clone();
        let resp = ureq::get(tag_url.as_str()).call()?;
        let json: serde_json::Value = resp.into_json().unwrap();

        Ok(format!("{}", json[0]["name"]).replace("\"", ""))
    }
    pub fn exec_match(&self, writer: &mut Writer, args: Option<&ArgMatches>) -> Result<()> {
        match args {
            Some(matches) => {
                let arch = self.get_architecture()?;
                writer.write_ok(format!("Arch {}", arch).as_str());

                let current_tag = if matches.value_of("version").unwrap() == "latest" {
                    self.get_latest_tag()?
                } else {
                    matches.value_of("version").unwrap().to_string()
                };
                writer.write_ok(format!("Tag {}", current_tag).as_str());
                // Create download dir.
                let bin_download_dir = format!(
                    "{}/downloads/{}",
                    self.conf.databend_dir.clone(),
                    current_tag
                );
                fs::create_dir_all(bin_download_dir.clone()).unwrap();

                // Create bin dir.
                let bin_unpack_dir =
                    format!("{}/bin/{}", self.conf.databend_dir.clone(), current_tag);
                fs::create_dir_all(bin_unpack_dir.clone()).unwrap();

                let bin_name = format!("databend-{}-{}.tar.gz", current_tag, arch);
                let bin_file = format!("{}/{}", bin_download_dir, bin_name);
                let exists = Path::new(bin_file.as_str()).exists();
                // Download.
                if !exists {
                    let binary_url = format!(
                        "{}/{}/{}",
                        self.conf.download_url.clone(),
                        current_tag,
                        bin_name,
                    );
                    let res = ureq::get(binary_url.as_str()).call()?;
                    let total_size: u64 = res.header("content-length").unwrap().parse().unwrap();
                    let pb = ProgressBar::new(total_size);
                    pb.set_style(ProgressStyle::default_bar()
                        .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                        .progress_chars("#>-"));

                    let mut out = File::create(bin_file.clone()).unwrap();
                    io::copy(&mut pb.wrap_read(res.into_reader()), &mut out).unwrap();
                    writer.write_ok(format!("Download {}", binary_url).as_str());
                }
                writer.write_ok(format!("Binary {}", bin_file).as_str());

                // Unpack.
                let tar_gz = File::open(bin_file)?;
                let tar = GzDecoder::new(tar_gz);
                let mut archive = Archive::new(tar);
                let res = archive.unpack(bin_unpack_dir.clone());
                match res {
                    Ok(_) => {
                        writer.write_ok(format!("Unpack {}", bin_unpack_dir).as_str());

                        // switch to fetched version
                        let switch = SwitchCommand::create(self.conf.clone());
                        return switch.exec_match(writer, args);
                    }
                    Err(e) => {
                        writer.write_err(format!("{}", e).as_str());
                        return Ok(());
                    }
                };
            }
            None => {
                println!("none ");
            }
        }

        Ok(())
    }
}
