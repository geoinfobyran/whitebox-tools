/*
This tool is part of the WhiteboxTools geospatial analysis library.
Authors: Dr. John Lindsay
Created: 26/016/2017
Last Modified: 18/10/2019
License: MIT
*/

use crate::raster::*;
use crate::structures::Array2D;
use crate::tools::*;
use num_cpus;
use std::env;
use std::f64;
use std::io::{Error, ErrorKind};
use std::path;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

/// This tool is used to generate a flow accumulation grid (i.e. catchment area) using the 
/// D8 (O'Callaghan and Mark, 1984) algorithm. This algorithm is an example of single-flow-direction 
/// (SFD) method because the flow entering each grid cell is routed to only one downslope neighbour, 
/// i.e. flow divergence is not permitted. The user must specify the name of the input digital 
/// elevation model (DEM). The DEM must have been hydrologically corrected to remove all spurious 
/// depressions and flat areas. DEM pre-processing is usually achieved using the `BreachDepressions` or 
/// `FillDepressions` tools.
/// 
/// In addition to the input DEM, the user must specify the output type. The output flow-accumulation 
/// can be 1) `cells` (i.e. the number of inflowing grid cells), `catchment area` (i.e. the upslope area),
/// or `specific contributing area` (i.e. the catchment area divided by the flow width. The default value
/// is `cells`. The user must also specify whether the output flow-accumulation grid should be 
/// log-tranformed (`--log`), i.e. the output, if this option is selected, will be the natural-logarithm of the 
/// accumulated flow value. This is a transformation that is often performed to better visualize the 
/// contributing area distribution. Because contributing areas tend to be very high along valley bottoms 
/// and relatively low on hillslopes, when a flow-accumulation image is displayed, the distribution of 
/// values on hillslopes tends to be 'washed out' because the palette is stretched out to represent the 
/// highest values. Log-transformation provides a means of compensating for this phenomenon. Importantly, 
/// however, log-transformed flow-accumulation grids must not be used to estimate other secondary terrain 
/// indices, such as the wetness index, or relative stream power index. 
/// 
/// Grid cells possessing the **NoData** value in the input flow-pointer grid are assigned the **NoData** 
/// value in the output flow-accumulation image.
/// 
/// # See Also:
/// `DInfFlowAccumulation`, `BreachDepressions`, `FillDepressions`
pub struct D8FlowAccumulation {
    name: String,
    description: String,
    toolbox: String,
    parameters: Vec<ToolParameter>,
    example_usage: String,
}

impl D8FlowAccumulation {
    pub fn new() -> D8FlowAccumulation {
        // public constructor
        let name = "D8FlowAccumulation".to_string();
        let toolbox = "Hydrological Analysis".to_string();
        let description = "Calculates a D8 flow accumulation raster from an input DEM.".to_string();

        let mut parameters = vec![];
        parameters.push(ToolParameter {
            name: "Input DEM File".to_owned(),
            flags: vec!["-i".to_owned(), "--dem".to_owned()],
            description: "Input raster DEM file.".to_owned(),
            parameter_type: ParameterType::ExistingFile(ParameterFileType::Raster),
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

        parameters.push(ToolParameter{
            name: "Output Type".to_owned(), 
            flags: vec!["--out_type".to_owned()], 
            description: "Output type; one of 'cells' (default), 'catchment area', and 'specific contributing area'.".to_owned(),
            parameter_type: ParameterType::OptionList(vec!["cells".to_owned(), "catchment area".to_owned(), "specific contributing area".to_owned()]),
            default_value: Some("cells".to_owned()),
            optional: true
        });

        parameters.push(ToolParameter {
            name: "Log-transform the output?".to_owned(),
            flags: vec!["--log".to_owned()],
            description: "Optional flag to request the output be log-transformed.".to_owned(),
            parameter_type: ParameterType::Boolean,
            default_value: None,
            optional: true,
        });

        parameters.push(ToolParameter {
            name: "Clip the upper tail by 1%?".to_owned(),
            flags: vec!["--clip".to_owned()],
            description: "Optional flag to request clipping the display max by 1%.".to_owned(),
            parameter_type: ParameterType::Boolean,
            default_value: None,
            optional: true,
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
        let usage = format!(">>.*{0} -r={1} -v --wd=\"*path*to*data*\" --dem=DEM.tif -o=output.tif --out_type='cells'
>>.*{0} -r={1} -v --wd=\"*path*to*data*\" --dem=DEM.tif -o=output.tif --out_type='specific catchment area' --log --clip", short_exe, name).replace("*", &sep);

        D8FlowAccumulation {
            name: name,
            description: description,
            toolbox: toolbox,
            parameters: parameters,
            example_usage: usage,
        }
    }
}

impl WhiteboxTool for D8FlowAccumulation {
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
        match serde_json::to_string(&self.parameters) {
            Ok(json_str) => return format!("{{\"parameters\":{}}}", json_str),
            Err(err) => return format!("{:?}", err),
        }
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
        let mut input_file = String::new();
        let mut output_file = String::new();
        let mut out_type = String::from("sca");
        let mut log_transform = false;
        let mut clip_max = false;

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
            if vec[0].to_lowercase() == "-i"
                || vec[0].to_lowercase() == "--input"
                || vec[0].to_lowercase() == "--dem"
            {
                if keyval {
                    input_file = vec[1].to_string();
                } else {
                    input_file = args[i + 1].to_string();
                }
            } else if vec[0].to_lowercase() == "-o" || vec[0].to_lowercase() == "--output" {
                if keyval {
                    output_file = vec[1].to_string();
                } else {
                    output_file = args[i + 1].to_string();
                }
            } else if vec[0].to_lowercase() == "-out_type" || vec[0].to_lowercase() == "--out_type"
            {
                if keyval {
                    out_type = vec[1].to_lowercase();
                } else {
                    out_type = args[i + 1].to_lowercase();
                }
                if out_type.contains("specific") || out_type.contains("sca") {
                    out_type = String::from("sca");
                } else if out_type.contains("cells") {
                    out_type = String::from("cells");
                } else {
                    out_type = String::from("ca");
                }
            } else if vec[0].to_lowercase() == "-log" || vec[0].to_lowercase() == "--log" {
                if vec.len() == 1 || !vec[1].to_string().to_lowercase().contains("false") {
                    log_transform = true;
                }
            } else if vec[0].to_lowercase() == "-clip" || vec[0].to_lowercase() == "--clip" {
                if vec.len() == 1 || !vec[1].to_string().to_lowercase().contains("false") {
                    clip_max = true;
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

        if !input_file.contains(&sep) && !input_file.contains("/") {
            input_file = format!("{}{}", working_directory, input_file);
        }
        if !output_file.contains(&sep) && !output_file.contains("/") {
            output_file = format!("{}{}", working_directory, output_file);
        }

        if verbose {
            println!("Reading data...")
        };

        let input = Arc::new(Raster::new(&input_file, "r")?);

        // calculate the flow direction
        let start = Instant::now();
        let rows = input.configs.rows as isize;
        let columns = input.configs.columns as isize;
        let num_cells = rows * columns;
        let nodata = input.configs.nodata;
        let cell_size_x = input.configs.resolution_x;
        let cell_size_y = input.configs.resolution_y;
        let diag_cell_size = (cell_size_x * cell_size_x + cell_size_y * cell_size_y).sqrt();

        let mut flow_dir: Array2D<i8> = Array2D::new(rows, columns, -1, -1)?;
        let num_procs = num_cpus::get() as isize;
        let (tx, rx) = mpsc::channel();
        for tid in 0..num_procs {
            let input = input.clone();
            let tx = tx.clone();
            thread::spawn(move || {
                let nodata = input.configs.nodata;
                let dx = [1, 1, 1, 0, -1, -1, -1, 0];
                let dy = [-1, 0, 1, 1, 1, 0, -1, -1];
                let grid_lengths = [
                    diag_cell_size,
                    cell_size_x,
                    diag_cell_size,
                    cell_size_y,
                    diag_cell_size,
                    cell_size_x,
                    diag_cell_size,
                    cell_size_y,
                ];
                let (mut z, mut z_n): (f64, f64);
                let (mut max_slope, mut slope): (f64, f64);
                let mut dir: i8;
                let mut neighbouring_nodata: bool;
                let mut interior_pit_found = false;
                for row in (0..rows).filter(|r| r % num_procs == tid) {
                    let mut data: Vec<i8> = vec![-1i8; columns as usize];
                    for col in 0..columns {
                        z = input[(row, col)];
                        if z != nodata {
                            dir = 0i8;
                            max_slope = f64::MIN;
                            neighbouring_nodata = false;
                            for i in 0..8 {
                                z_n = input[(row + dy[i], col + dx[i])];
                                if z_n != nodata {
                                    slope = (z - z_n) / grid_lengths[i];
                                    if slope > max_slope && slope > 0f64 {
                                        max_slope = slope;
                                        dir = i as i8;
                                    }
                                } else {
                                    neighbouring_nodata = true;
                                }
                            }
                            if max_slope >= 0f64 {
                                data[col as usize] = dir;
                            } else {
                                data[col as usize] = -1i8;
                                if !neighbouring_nodata {
                                    interior_pit_found = true;
                                }
                            }
                        } else {
                            data[col as usize] = -1i8;
                        }
                    }
                    tx.send((row, data, interior_pit_found)).unwrap();
                }
            });
        }

        let mut interior_pit_found = false;
        for r in 0..rows {
            let (row, data, pit) = rx.recv().unwrap();
            flow_dir.set_row_data(row, data); //(data.0, data.1);
            if pit {
                interior_pit_found = true;
            }
            if verbose {
                progress = (100.0_f64 * r as f64 / (rows - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Flow directions: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        // calculate the number of inflowing cells
        let flow_dir = Arc::new(flow_dir);
        let mut num_inflowing: Array2D<i8> = Array2D::new(rows, columns, -1, -1)?;

        let (tx, rx) = mpsc::channel();
        for tid in 0..num_procs {
            let input = input.clone();
            let flow_dir = flow_dir.clone();
            let tx = tx.clone();
            thread::spawn(move || {
                let dx = [1, 1, 1, 0, -1, -1, -1, 0];
                let dy = [-1, 0, 1, 1, 1, 0, -1, -1];
                let inflowing_vals: [i8; 8] = [4, 5, 6, 7, 0, 1, 2, 3];
                let mut z: f64;
                let mut count: i8;
                for row in (0..rows).filter(|r| r % num_procs == tid) {
                    let mut data: Vec<i8> = vec![-1i8; columns as usize];
                    for col in 0..columns {
                        z = input[(row, col)];
                        if z != nodata {
                            count = 0i8;
                            for i in 0..8 {
                                if flow_dir[(row + dy[i], col + dx[i])] == inflowing_vals[i] {
                                    count += 1;
                                }
                            }
                            data[col as usize] = count;
                        } else {
                            data[col as usize] = -1i8;
                        }
                    }
                    tx.send((row, data)).unwrap();
                }
            });
        }

        let mut output = Raster::initialize_using_file(&output_file, &input);
        output.reinitialize_values(1.0);
        let mut stack = Vec::with_capacity((rows * columns) as usize);
        let mut num_solved_cells = 0;
        for r in 0..rows {
            let (row, data) = rx.recv().unwrap();
            num_inflowing.set_row_data(row, data);
            for col in 0..columns {
                if num_inflowing[(row, col)] == 0i8 {
                    stack.push((row, col));
                } else if num_inflowing[(row, col)] == -1i8 {
                    num_solved_cells += 1;
                }
            }

            if verbose {
                progress = (100.0_f64 * r as f64 / (rows - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Num. inflowing neighbours: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        let dx = [1, 1, 1, 0, -1, -1, -1, 0];
        let dy = [-1, 0, 1, 1, 1, 0, -1, -1];
        let (mut row, mut col): (isize, isize);
        let (mut row_n, mut col_n): (isize, isize);
        // let mut cell: (isize, isize);
        let mut dir: i8;
        let mut fa: f64;
        while !stack.is_empty() {
            let cell = stack.pop().unwrap();
            row = cell.0;
            col = cell.1;
            fa = output[(row, col)];
            num_inflowing.decrement(row, col, 1i8);
            dir = flow_dir[(row, col)];
            if dir >= 0 {
                row_n = row + dy[dir as usize];
                col_n = col + dx[dir as usize];
                output.increment(row_n, col_n, fa);
                num_inflowing.decrement(row_n, col_n, 1i8);
                if num_inflowing.get_value(row_n, col_n) == 0i8 {
                    stack.push((row_n, col_n));
                }
            }

            if verbose {
                num_solved_cells += 1;
                progress = (100.0_f64 * num_solved_cells as f64 / (num_cells - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Flow accumulation: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        let mut cell_area = cell_size_x * cell_size_y;
        // if flow width is allowed to vary by direction, the flow accumulation output will not
        // increase continuously downstream and any applications involving stream network
        // extraction will encounter issues with discontinuous streams. The Whitebox GAT tool
        // used a constant flow width value. I'm reverting this tool to the equivalent.
        // let mut flow_widths = [
        //     diag_cell_size,
        //     cell_size_y,
        //     diag_cell_size,
        //     cell_size_x,
        //     diag_cell_size,
        //     cell_size_y,
        //     diag_cell_size,
        //     cell_size_x,
        // ];

        let avg_cell_size = (cell_size_x + cell_size_y) / 2.0;
        let mut flow_widths = [
            avg_cell_size,
            avg_cell_size,
            avg_cell_size,
            avg_cell_size,
            avg_cell_size,
            avg_cell_size,
            avg_cell_size,
            avg_cell_size,
        ];
        if out_type == "cells" {
            cell_area = 1.0;
            flow_widths = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        } else if out_type == "ca" {
            flow_widths = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        }

        if log_transform {
            for row in 0..rows {
                for col in 0..columns {
                    if input[(row, col)] == nodata {
                        output[(row, col)] = nodata;
                    } else {
                        let dir = flow_dir[(row, col)];
                        if dir >= 0 {
                            output[(row, col)] =
                                (output[(row, col)] * cell_area / flow_widths[dir as usize]).ln();
                        } else {
                            output[(row, col)] =
                                (output[(row, col)] * cell_area / flow_widths[3]).ln();
                        }
                    }
                }

                if verbose {
                    progress = (100.0_f64 * row as f64 / (rows - 1) as f64) as usize;
                    if progress != old_progress {
                        println!("Correcting values: {}%", progress);
                        old_progress = progress;
                    }
                }
            }
        } else {
            for row in 0..rows {
                for col in 0..columns {
                    if input[(row, col)] == nodata {
                        output[(row, col)] = nodata;
                    } else {
                        let dir = flow_dir[(row, col)];
                        if dir >= 0 {
                            output[(row, col)] =
                                output[(row, col)] * cell_area / flow_widths[dir as usize];
                        } else {
                            output[(row, col)] = output[(row, col)] * cell_area / flow_widths[3];
                        }
                    }
                }

                if verbose {
                    progress = (100.0_f64 * row as f64 / (rows - 1) as f64) as usize;
                    if progress != old_progress {
                        println!("Correcting values: {}%", progress);
                        old_progress = progress;
                    }
                }
            }
        }

        output.configs.palette = "blueyellow.plt".to_string();
        if clip_max {
            output.clip_display_max(1.0);
        }
        let elapsed_time = get_formatted_elapsed_time(start);
        output.add_metadata_entry(format!(
            "Created by whitebox_tools\' {} tool",
            self.get_tool_name()
        ));
        output.add_metadata_entry(format!("Input file: {}", input_file));
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
        if interior_pit_found {
            println!("**********************************************************************************");
            println!("WARNING: Interior pit cells were found within the input DEM. It is likely that the 
            DEM needs to be processed to remove topographic depressions and flats prior to
            running this tool.");
            println!("**********************************************************************************");
        }

        Ok(())
    }
}
