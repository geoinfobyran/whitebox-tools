/*
This tool is part of the WhiteboxTools geospatial analysis library.
Authors: Dr. John Lindsay
Created: 06/07/2017
Last Modified: 13/10/2018
License: MIT
*/

use crate::raster::*;
use crate::tools::*;
use num_cpus;
use std::env;
use std::f64;
use std::io::{Error, ErrorKind};
use std::path;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

/// This tool creates a new raster (`--output`) in which each grid cell is calculated as 
/// a value (`--input1`) raised to the power of another value (`--input2`). Both inputs can either be
/// rasters or constant values. Moderate to exponent large values will result in very large output values 
/// and this may cause errors when you display the data. Grid cells with **NoData** values in either of the input 
/// rasters will be assigned **NoData** values in the output raster.
/// 
/// # See Also
/// `Exp`, `Exp2`
pub struct Power {
    name: String,
    description: String,
    toolbox: String,
    parameters: Vec<ToolParameter>,
    example_usage: String,
}

impl Power {
    /// public constructor
    pub fn new() -> Power {
        let name = "Power".to_string();
        let toolbox = "Math and Stats Tools".to_string();
        let description = "Raises the values in grid cells of one rasters, or a constant value, by values in another raster or constant value.".to_string();

        let mut parameters = vec![];
        parameters.push(ToolParameter {
            name: "Input File Or Constant Value".to_owned(),
            flags: vec!["--input1".to_owned()],
            description: "Input raster file or constant value.".to_owned(),
            parameter_type: ParameterType::ExistingFileOrFloat(ParameterFileType::Raster),
            default_value: None,
            optional: false,
        });

        parameters.push(ToolParameter {
            name: "Input File Or Constant Value".to_owned(),
            flags: vec!["--input2".to_owned()],
            description: "Input raster file or constant value.".to_owned(),
            parameter_type: ParameterType::ExistingFileOrFloat(ParameterFileType::Raster),
            default_value: None,
            optional: false,
        });

        parameters.push(ToolParameter {
            name: "Output File".to_owned(),
            flags: vec!["-o".to_owned(), "--output".to_owned()],
            description: "Output raster file.".to_owned(),
            parameter_type: ParameterType::NewFile(ParameterFileType::Raster),
            default_value: None,
            optional: false,
        });

        let sep: String = path::MAIN_SEPARATOR.to_string();
        let p = format!("{}", env::current_dir().unwrap().display());
        let e = format!("{}", env::current_exe().unwrap().display());
        let mut short_exe = e
            .replace(&p, "")
            .replace(".exe", "")
            .replace(".", "")
            .replace(&sep, "");
        if e.contains(".exe") {
            short_exe += ".exe";
        }
        let usage = format!(">>.*{0} -r={1} -v --wd=\"*path*to*data*\" --input1='in1.tif' --input2='in2.tif' -o=output.tif", short_exe, name).replace("*", &sep);

        Power {
            name: name,
            description: description,
            toolbox: toolbox,
            parameters: parameters,
            example_usage: usage,
        }
    }
}

impl WhiteboxTool for Power {
    fn get_source_file(&self) -> String {
        String::from(file!())
    }

    fn get_tool_name(&self) -> String {
        self.name.clone()
    }

    fn get_tool_description(&self) -> String {
        self.description.clone()
    }

    fn get_tool_parameters(&self) -> String {
        let mut s = String::from("{\"parameters\": [");
        for i in 0..self.parameters.len() {
            if i < self.parameters.len() - 1 {
                s.push_str(&(self.parameters[i].to_string()));
                s.push_str(",");
            } else {
                s.push_str(&(self.parameters[i].to_string()));
            }
        }
        s.push_str("]}");
        s
    }

    fn get_example_usage(&self) -> String {
        self.example_usage.clone()
    }

    fn get_toolbox(&self) -> String {
        self.toolbox.clone()
    }

    fn run<'a>(
        &self,
        args: Vec<String>,
        working_directory: &'a str,
        verbose: bool,
    ) -> Result<(), Error> {
        let mut input1 = String::new();
        let mut input2 = String::new();
        let mut output_file = String::new();

        if args.len() == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Tool run with no paramters.",
            ));
        }
        for i in 0..args.len() {
            let mut arg = args[i].replace("\"", "");
            arg = arg.replace("\'", "");
            let cmd = arg.split("="); // in case an equals sign was used
            let vec = cmd.collect::<Vec<&str>>();
            let mut keyval = false;
            if vec.len() > 1 {
                keyval = true;
            }
            if vec[0].to_lowercase() == "-i1" || vec[0].to_lowercase() == "--input1" {
                if keyval {
                    input1 = vec[1].to_string();
                } else {
                    input1 = args[i + 1].to_string();
                }
            } else if vec[0].to_lowercase() == "-i2" || vec[0].to_lowercase() == "--input2" {
                if keyval {
                    input2 = vec[1].to_string();
                } else {
                    input2 = args[i + 1].to_string();
                }
            } else if vec[0].to_lowercase() == "-o" || vec[0].to_lowercase() == "--output" {
                if keyval {
                    output_file = vec[1].to_string();
                } else {
                    output_file = args[i + 1].to_string();
                }
            }
        }

        if verbose {
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
            println!("* Welcome to {} *", self.get_tool_name());
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
        }

        let sep: String = path::MAIN_SEPARATOR.to_string();

        let mut progress: usize;
        let mut old_progress: usize = 1;

        if !output_file.contains(&sep) && !output_file.contains("/") {
            output_file = format!("{}{}", working_directory, output_file);
        }

        // Are either of the inputs constants?
        let mut input1_constant = f64::NEG_INFINITY;
        let input1_is_constant = match input1.parse::<f64>() {
            Ok(val) => {
                input1_constant = val;
                true
            }
            Err(_) => false,
        };
        if !input1_is_constant {
            if !input1.contains(&sep) && !input1.contains("/") {
                input1 = format!("{}{}", working_directory, input1);
            }
        }

        let mut input2_constant = f64::NEG_INFINITY;
        let input2_is_constant = match input2.parse::<f64>() {
            Ok(val) => {
                input2_constant = val;
                true
            }
            Err(_) => false,
        };
        if !input2_is_constant {
            if !input2.contains(&sep) && !input2.contains("/") {
                input2 = format!("{}{}", working_directory, input2);
            }
        }

        if input1_is_constant && input2_is_constant {
            // return Err(Error::new(ErrorKind::InvalidInput,
            //                     "At least one of the inputs must be a raster."));
            println!("{}", input1_constant.powf(input2_constant));
            return Ok(());
        } else if input1_is_constant && !input2_is_constant {
            if verbose {
                println!("Reading data...")
            };
            let in2 = Arc::new(Raster::new(&input2, "r")?);

            let start = Instant::now();
            let rows = in2.configs.rows as isize;
            let columns = in2.configs.columns as isize;
            let nodata2 = in2.configs.nodata;

            let num_procs = num_cpus::get() as isize;
            let (tx, rx) = mpsc::channel();
            for tid in 0..num_procs {
                let in2 = in2.clone();
                let tx = tx.clone();
                thread::spawn(move || {
                    let mut z2: f64;
                    for row in (0..rows).filter(|r| r % num_procs == tid) {
                        let mut data: Vec<f64> = vec![nodata2; columns as usize];
                        for col in 0..columns {
                            z2 = in2[(row, col)];
                            if z2 != nodata2 {
                                data[col as usize] = input1_constant.powf(z2);
                            } else {
                                data[col as usize] = nodata2;
                            }
                        }
                        tx.send((row, data)).unwrap();
                    }
                });
            }

            let mut output = Raster::initialize_using_file(&output_file, &in2);
            for r in 0..rows {
                let (row, data) = rx.recv().unwrap();
                output.set_row_data(row, data);

                if verbose {
                    progress = (100.0_f64 * r as f64 / (rows - 1) as f64) as usize;
                    if progress != old_progress {
                        println!("Progress: {}%", progress);
                        old_progress = progress;
                    }
                }
            }

            let elapsed_time = get_formatted_elapsed_time(start);
            output.add_metadata_entry(format!(
                "Created by whitebox_tools\' {} tool",
                self.get_tool_name()
            ));
            output.add_metadata_entry(format!("Elapsed Time (excluding I/O): {}", elapsed_time));

            if verbose {
                println!("Saving data...")
            };
            let _ = match output.write() {
                Ok(_) => {
                    if verbose {
                        println!("Output file written")
                    }
                }
                Err(e) => return Err(e),
            };
            if verbose {
                println!(
                    "{}",
                    &format!("Elapsed Time (excluding I/O): {}", elapsed_time)
                );
            }
        } else if !input1_is_constant && input2_is_constant {
            if verbose {
                println!("Reading data...")
            };
            let in1 = Arc::new(Raster::new(&input1, "r")?);

            let start = Instant::now();
            let rows = in1.configs.rows as isize;
            let columns = in1.configs.columns as isize;
            let nodata1 = in1.configs.nodata;

            let num_procs = num_cpus::get() as isize;
            let (tx, rx) = mpsc::channel();
            for tid in 0..num_procs {
                let in1 = in1.clone();
                let tx = tx.clone();
                thread::spawn(move || {
                    let mut z1: f64;
                    for row in (0..rows).filter(|r| r % num_procs == tid) {
                        let mut data: Vec<f64> = vec![nodata1; columns as usize];
                        for col in 0..columns {
                            z1 = in1[(row, col)];
                            if z1 != nodata1 {
                                data[col as usize] = z1.powf(input2_constant);
                            } else {
                                data[col as usize] = nodata1;
                            }
                        }
                        tx.send((row, data)).unwrap();
                    }
                });
            }

            let mut output = Raster::initialize_using_file(&output_file, &in1);
            for r in 0..rows {
                let (row, data) = rx.recv().unwrap();
                output.set_row_data(row, data);

                if verbose {
                    progress = (100.0_f64 * r as f64 / (rows - 1) as f64) as usize;
                    if progress != old_progress {
                        println!("Progress: {}%", progress);
                        old_progress = progress;
                    }
                }
            }

            let elapsed_time = get_formatted_elapsed_time(start);
            output.add_metadata_entry(format!(
                "Created by whitebox_tools\' {} tool",
                self.get_tool_name()
            ));
            output.add_metadata_entry(format!("Elapsed Time (excluding I/O): {}", elapsed_time));

            if verbose {
                println!("Saving data...")
            };
            let _ = match output.write() {
                Ok(_) => {
                    if verbose {
                        println!("Output file written")
                    }
                }
                Err(e) => return Err(e),
            };
            if verbose {
                println!(
                    "{}",
                    &format!("Elapsed Time (excluding I/O): {}", elapsed_time)
                );
            }
        } else {
            // !input1_is_constant && !input2_is_constant
            if verbose {
                println!("Reading data...")
            };
            let in1 = Arc::new(Raster::new(&input1, "r")?);
            let in2 = Arc::new(Raster::new(&input2, "r")?);

            let start = Instant::now();
            let rows = in1.configs.rows as isize;
            let columns = in1.configs.columns as isize;
            let nodata1 = in1.configs.nodata;
            let nodata2 = in2.configs.nodata;

            // make sure the input files have the same size
            if in1.configs.rows != in2.configs.rows || in1.configs.columns != in2.configs.columns {
                return Err(Error::new(ErrorKind::InvalidInput,
                                    "The input files must have the same number of rows and columns and spatial extent."));
            }

            let num_procs = num_cpus::get() as isize;
            let (tx, rx) = mpsc::channel();
            for tid in 0..num_procs {
                let in1 = in1.clone();
                let in2 = in2.clone();
                let tx = tx.clone();
                thread::spawn(move || {
                    let mut z1: f64;
                    let mut z2: f64;
                    for row in (0..rows).filter(|r| r % num_procs == tid) {
                        let mut data: Vec<f64> = vec![nodata1; columns as usize];
                        for col in 0..columns {
                            z1 = in1[(row, col)];
                            z2 = in2[(row, col)];
                            if z1 != nodata1 && z2 != nodata2 {
                                data[col as usize] = z1.powf(z2);
                            } else {
                                data[col as usize] = nodata1;
                            }
                        }
                        tx.send((row, data)).unwrap();
                    }
                });
            }

            let mut output = Raster::initialize_using_file(&output_file, &in1);
            for r in 0..rows {
                let (row, data) = rx.recv().unwrap();
                output.set_row_data(row, data);

                if verbose {
                    progress = (100.0_f64 * r as f64 / (rows - 1) as f64) as usize;
                    if progress != old_progress {
                        println!("Progress: {}%", progress);
                        old_progress = progress;
                    }
                }
            }

            let elapsed_time = get_formatted_elapsed_time(start);
            output.add_metadata_entry(format!(
                "Created by whitebox_tools\' {} tool",
                self.get_tool_name()
            ));
            output.add_metadata_entry(format!("Elapsed Time (excluding I/O): {}", elapsed_time));

            if verbose {
                println!("Saving data...")
            };
            let _ = match output.write() {
                Ok(_) => {
                    if verbose {
                        println!("Output file written")
                    }
                }
                Err(e) => return Err(e),
            };
            if verbose {
                println!(
                    "{}",
                    &format!("Elapsed Time (excluding I/O): {}", elapsed_time)
                );
            }
        }

        Ok(())
    }
}
