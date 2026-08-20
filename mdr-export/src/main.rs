use anyhow::{Result, anyhow, bail};
use clap::Parser;
use diesel::pg::PgConnection;
use dotenvy::dotenv;
use libmdrepo::{constants, metadata};
use mdr_db::ops;
use mdr_export::types::{Args, FileFormat, Server};
use std::{
    collections::HashSet,
    env,
    fs::{self, File},
    io::Write,
};

const DEFAULT_ORCID: &str = "0000-0000-0000-0000";

// --------------------------------------------------
fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

// --------------------------------------------------
fn run(args: Args) -> Result<()> {
    dotenv().ok();
    let env_key = match args.server {
        Server::Production => "PRODUCTION_DSN",
        Server::Staging => "STAGING_DSN",
    };
    let url = match env::var(env_key) {
        Ok(val) => val,
        Err(e) => bail!("{env_key}: {e}"),
    };
    let out_dir = &args.out_dir;
    if !out_dir.exists() {
        fs::create_dir(out_dir)?;
    }

    let mut conn = mdr_db::connect(&url)?;

    // public-json is a promise about which rows are exported, not only about
    // the shape of each one: a "public record" describing an embargoed
    // simulation is a leak whatever fields it carries. So the format selects
    // the publicly visible set, and narrows an explicit --simulation-ids list
    // to it as well. Exempting named IDs would make the guarantee hold only
    // for whole-database exports -- which is precisely the case where someone
    // would then assume it holds everywhere.
    let public_only = args.format == FileFormat::PublicJson;
    let simulation_ids = if !args.simulation_ids.is_empty() {
        if public_only {
            keep_visible(&mut conn, args.simulation_ids)?
        } else {
            args.simulation_ids
        }
    } else if public_only {
        ops::get_visible_simulation_ids(&mut conn)?
    } else {
        ops::get_all_simulation_ids(&mut conn)?
    };

    for (i, sim_id) in simulation_ids.into_iter().enumerate() {
        let mdrepo_id = format!("MDR{sim_id:08}");
        let out_file = out_dir.join(format!("{}.{}", mdrepo_id, args.format));
        print!("{:6}: {mdrepo_id}", i + 1,);

        if !out_file.exists() || args.overwrite {
            println!();
            let content = if args.format == FileFormat::PublicJson {
                get_sim_summary(&mut conn, sim_id).and_then(|s| s.to_json())
            } else {
                get_sim(&mut conn, sim_id).and_then(|meta| {
                    if args.format == FileFormat::Json {
                        meta.to_json()
                    } else {
                        meta.to_toml()
                    }
                })
            };

            match content {
                Ok(content) => {
                    let mut out_fh = File::create(&out_file)?;
                    write!(out_fh, "{content}")?;
                }
                Err(e) => {
                    eprintln!("{e}");
                }
            }
        } else {
            println!(" outfile exists, skipping");
        }
    }

    Ok(())
}

// --------------------------------------------------
/// Narrow an explicitly requested ID list to the publicly visible ones,
/// naming anything dropped.
fn keep_visible(conn: &mut PgConnection, requested: Vec<i64>) -> Result<Vec<i64>> {
    let visible: HashSet<i64> =
        ops::get_visible_simulation_ids(conn)?.into_iter().collect();
    let (keep, dropped): (Vec<i64>, Vec<i64>) =
        requested.into_iter().partition(|id| visible.contains(id));

    if !dropped.is_empty() {
        // Loud, and by name. The alternative is an empty output directory and
        // no stated reason, which reads as a broken exporter rather than as
        // the format doing its job.
        let names: Vec<String> =
            dropped.iter().map(|id| format!("MDR{id:08}")).collect();
        eprintln!(
            "Skipping {} simulation(s) not publicly visible: {}",
            dropped.len(),
            names.join(", ")
        );
    }

    Ok(keep)
}

// --------------------------------------------------
fn empty_to_none(val: Option<String>) -> Option<String> {
    val.filter(|v| !v.is_empty())
}

// --------------------------------------------------
fn get_sim(conn: &mut PgConnection, sim_id: i64) -> Result<metadata::Meta> {
    let sim = ops::get_simulation(conn, sim_id)?;
    let software = ops::get_software(
        conn,
        sim.software_id
            .ok_or_else(|| anyhow!("simulation has no software_id"))?,
    )?;
    let pdb = sim
        .pdb_id
        .map(|pdb_pk| ops::get_pdb(conn, pdb_pk))
        .transpose()?;
    let (_, sim2uniprot) =
        ops::list_simulation_uniprots(conn, Some(sim_id), None, true, None, None)?;
    let mut uniprot_ids: Vec<String> = vec![];
    for s2u in sim2uniprot {
        uniprot_ids.push(
            ops::get_uniprot(conn, s2u.uniprot_id)?
                .uniprot_id
                .to_string(),
        );
    }

    let water = match (&sim.water_type, &sim.water_density) {
        (Some(model), Some(density_kg_m3)) => Some(metadata::Water {
            model: model.clone(),
            density_kg_m3: *density_kg_m3,
        }),
        _ => None,
    };

    let lead_contributor_orcid = match sim.created_by_id {
        Some(user_id) => {
            let (_count, res) = ops::list_social_accounts(
                conn,
                Some("orcid".to_string()),
                Some(user_id),
                true,
                None,
                None,
            )?;
            res.first().map(|v| v.uid.to_string())
        }
        _ => None,
    };

    let (_, up_files) =
        ops::list_uploaded_files(conn, None, Some(sim_id), true, None, None)?;

    let mut additional_files: Vec<metadata::AdditionalFile> = vec![];
    let mut trajectory_file_names: Vec<String> = vec![];
    let mut structure_file_name: Option<String> = None;
    let mut topology_file_name: Option<String> = None;

    for file in up_files {
        if file.is_primary {
            match file.file_type.as_str() {
                "Trajectory" => {
                    trajectory_file_names.push(file.filename.clone());
                }
                "Structure" => {
                    if structure_file_name.is_none() {
                        structure_file_name = Some(file.filename.clone())
                    } else {
                        bail!("Found multiple primary structure files");
                    }
                }
                "Topology" => {
                    if topology_file_name.is_none() {
                        topology_file_name = Some(file.filename.clone())
                    } else {
                        bail!("Found multiple primary topology files");
                    }
                }
                _ => bail!(
                    r#"Primary file type "{}" is not Trajectory/Structure/Topology"#,
                    file.file_type
                ),
            }
        } else {
            additional_files.push(metadata::AdditionalFile {
                file_name: file.filename.clone(),
                file_type: file.file_type.clone(),
                description: file.description.clone(),
            })
        }
    }

    for (field, val) in [
        ("structure_file_name", &structure_file_name),
        ("topology_file_name", &topology_file_name),
    ] {
        if val.is_none() {
            bail!(r#""{field}" has no value"#)
        }
    }

    let (_, replicates_res) =
        ops::list_replicates(conn, None, Some(sim_id), true, None, None)?;
    let replicates = replicates_res
        .into_iter()
        .map(|val| val.trajectory_file_name.to_string())
        .collect::<Vec<_>>();

    if trajectory_file_names.is_empty() {
        bail!(r#""trajectory_file_names" is empty"#)
    }

    let (_, ligands_res) =
        ops::list_ligands(conn, None, Some(sim_id), true, None, None)?;

    let ligands = (!ligands_res.is_empty()).then(|| {
        ligands_res
            .into_iter()
            .map(|val| metadata::Ligand {
                name: val.name,
                smiles: val.smiles_string,
            })
            .collect::<Vec<_>>()
    });

    let (_, solutes_res) =
        ops::list_solutes(conn, None, Some(sim_id), true, None, None)?;

    let solutes = (!solutes_res.is_empty()).then(|| {
        solutes_res
            .into_iter()
            .map(|val| metadata::Solute {
                name: val.name,
                concentration_mol_liter: val.concentration,
            })
            .collect::<Vec<_>>()
    });

    let (_, links_res) =
        ops::list_external_links(conn, None, Some(sim_id), true, None, None)?;

    let external_links = (!links_res.is_empty()).then(|| {
        links_res
            .into_iter()
            .map(|val| metadata::ExternalLink {
                url: val.url,
                label: val.label,
            })
            .collect::<Vec<_>>()
    });

    let (_, contributors_res) =
        ops::list_contributions(conn, None, Some(sim_id), true, None, None)?;

    let contributors = (!contributors_res.is_empty())
        .then(|| {
            contributors_res
                .into_iter()
                .map(|val| -> Result<metadata::Contributor> {
                    Ok(metadata::Contributor {
                        name: val
                            .name
                            .ok_or_else(|| anyhow!("contributor has no name"))?,
                        email: empty_to_none(val.email),
                        institution: empty_to_none(val.institution),
                        orcid: empty_to_none(val.orcid),
                    })
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;

    let (_, sim2pub) =
        ops::list_simulation_pubs(conn, Some(sim_id), None, true, None, None)?;
    let mut papers: Vec<metadata::Paper> = vec![];
    for s2p in sim2pub {
        let val = ops::get_pub(conn, s2p.pub_id)?;
        papers.push(metadata::Paper {
            title: val.title,
            authors: val.authors,
            journal: val.journal,
            volume: val.volume as u32,
            number: val.number,
            year: val.year as u32,
            pages: val.pages,
            doi: val.doi,
        });
    }

    let meta = metadata::Meta {
        lead_contributor_orcid: lead_contributor_orcid
            .unwrap_or(DEFAULT_ORCID.to_string()),
        trajectory_file_names: replicates,
        structure_file_name: structure_file_name
            .ok_or_else(|| anyhow!("simulation has no structure file"))?,
        topology_file_name: topology_file_name
            .ok_or_else(|| anyhow!("simulation has no topology file"))?,
        temperature_kelvin: sim
            .temperature
            .ok_or_else(|| anyhow!("simulation has no temperature"))?
            as u32,
        integration_timestep_fs: sim
            .integration_timestep_fs
            .ok_or_else(|| anyhow!("simulation has no integration timestep"))?
            as u32,
        // The DECLARED value, and it must never be sim.sampling_frequency.
        // That column is the *measured* one, derived from the converted XTC
        // and stored in nanoseconds; mapping it here would make every exported
        // record assert a declaration no submitter made -- and for exactly the
        // records this field exists to rescue it would assert the fabricated
        // 1 ps/frame cpptraj stamps on, which would then pass the ingest check
        // this field feeds. Null for almost every simulation, which is right:
        // a trajectory carrying its own time axis needs no declaration.
        sampling_frequency_ps: sim.sampling_frequency_ps,
        short_description: sim.short_description,
        software_name: software.name,
        software_version: software.version.unwrap_or_default(),
        mdrepo_id: Some(format!("MDR{sim_id:08}")),
        description: sim.description,
        toml_version: Some(constants::METADATA_TOML_VERSION),
        alias: sim.alias,
        run_commands: sim.run_commands,
        forcefield: sim.forcefield,
        forcefield_comments: sim.forcefield_comments,
        protonation_method: sim.protonation_method,
        pdb_id: pdb.map(|val| val.pdb_id),
        uniprot_ids: if uniprot_ids.is_empty() {
            None
        } else {
            Some(uniprot_ids)
        },
        water,
        additional_files: if additional_files.is_empty() {
            None
        } else {
            Some(additional_files)
        },
        ligands,
        solutes,
        dois: None,
        is_embargoed: None,
        papers: if papers.is_empty() {
            None
        } else {
            Some(papers)
        },
        external_links,
        contributors,
    };

    Ok(meta)
}

// --------------------------------------------------
fn get_sim_summary(conn: &mut PgConnection, sim_id: i64) -> Result<metadata::Summary> {
    let meta = get_sim(conn, sim_id)?;
    let sim = ops::get_simulation(conn, sim_id)?;

    Ok(metadata::Summary {
        id: sim.id,
        duration: sim.duration,
        num_replicates: sim.num_replicates,
        is_deprecated: sim.is_deprecated,
        is_placeholder: sim.is_placeholder,
        creation_date: sim.creation_date,
        superseding_simulation_id: sim.superseding_simulation_id,
        meta,
    })
}
