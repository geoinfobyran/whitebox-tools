/*
This tool is part of the WhiteboxTools geospatial analysis library.
Authors: Dr. John Lindsay
Created: 01/07/2017
Last Modified: 18/10/2019
License: MIT
*/

use crate::raster::*;
use crate::structures::Array2D;
use crate::tools::*;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::collections::VecDeque;
use std::env;
use std::f64;
use std::i32;
use std::io::{Error, ErrorKind};
use std::path;

/// This tool identifies each sink (i.e. topographic depression) in a raster digital elevation model (DEM). A 
/// sink, or depression, is a bowl-like landscape feature, which is characterized by interior drainage. Each 
/// identified sink in the input DEM is assigned a unique, non-zero, positive value in the ouput raster. The 
/// `Sink` tool essentially runs the `FillDepressions` tool followed by the `Clump` tool on all modified grid
/// cells.
/// 
/// # See Also
/// `FillDepressions`, `Clump`
pub struct Sink {
    name: String,
    description: String,
    toolbox: String,
    parameters: Vec<ToolParameter>,
    example_usage: String,
}

impl Sink {
    pub fn new() -> Sink {
        // public constructor
        let name = "Sink".to_string();
        let toolbox = "Hydrological Analysis".to_string();
        let description =
            "Identifies the depressions in a DEM, giving each feature a unique identifier."
                .to_string();

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

        parameters.push(ToolParameter {
            name: "Should a background value of zero be used?".to_owned(),
            flags: vec!["--zero_background".to_owned()],
            description: "Flag indicating whether a background value of zero should be used."
                .to_owned(),
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
        let usage = format!(">>.*{0} -r={1} -v --wd=\"*path*to*data*\" --dem=DEM.tif -o=output.tif --zero_background", short_exe, name).replace("*", &sep);

        Sink {
            name: name,
            description: description,
            toolbox: toolbox,
            parameters: parameters,
            example_usage: usage,
        }
    }
}

impl WhiteboxTool for Sink {
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
        let mut zero_background = false;

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
            let flag_val = vec[0].to_lowercase().replace("--", "-");
            if flag_val == "-i" || flag_val == "-input" || flag_val == "-dem" {
                input_file = if keyval {
                    vec[1].to_string()
                } else {
                    args[i + 1].to_string()
                };
            } else if flag_val == "-o" || flag_val == "-output" {
                output_file = if keyval {
                    vec[1].to_string()
                } else {
                    args[i + 1].to_string()
                };
            } else if flag_val == "-zero_background" {
                if vec.len() == 1 || !vec[1].to_string().to_lowercase().contains("false") {
                    zero_background = true;
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

        let input = Raster::new(&input_file, "r")?;

        let start = Instant::now();
        let rows = input.configs.rows as isize;
        let columns = input.configs.columns as isize;
        let num_cells = rows * columns;
        let nodata = input.configs.nodata;

        let mut output = Raster::initialize_using_file(&output_file, &input);
        let mut background_val = (i32::min_value() + 1) as f64;
        output.reinitialize_values(background_val);

        /*
        Find the data edges. This is complicated by the fact that DEMs frequently
        have nodata edges, whereby the DEM does not occupy the full extent of
        the raster. One approach to doing this would be simply to scan the
        raster, looking for cells that neighbour nodata values. However, this
        assumes that there are no interior nodata holes in the dataset. Instead,
        the approach used here is to perform a region-growing operation, looking
        for nodata values along the raster's edges.
        */

        let mut queue: VecDeque<(isize, isize)> =
            VecDeque::with_capacity((rows * columns) as usize);
        for row in 0..rows {
            /*
            Note that this is only possible because Whitebox rasters
            allow you to address cells beyond the raster extent but
            return the nodata value for these regions.
            */
            queue.push_back((row, -1));
            queue.push_back((row, columns));
        }

        for col in 0..columns {
            queue.push_back((-1, col));
            queue.push_back((rows, col));
        }

        /*
        minheap is the priority queue. Note that I've tested using integer-based
        priority values, by multiplying the elevations, but this didn't result
        in a significant performance gain over the use of f64s.
        */
        let mut minheap = BinaryHeap::with_capacity((rows * columns) as usize);
        let mut num_solved_cells = 0;
        let mut zin_n: f64; // value of neighbour of row, col in input raster
        let mut zout: f64; // value of row, col in output raster
        let mut zout_n: f64; // value of neighbour of row, col in output raster
        let dx = [1, 1, 1, 0, -1, -1, -1, 0];
        let dy = [-1, 0, 1, 1, 1, 0, -1, -1];
        let (mut row, mut col): (isize, isize);
        let (mut row_n, mut col_n): (isize, isize);
        while !queue.is_empty() {
            let cell = queue.pop_front().unwrap();
            row = cell.0;
            col = cell.1;
            for n in 0..8 {
                row_n = row + dy[n];
                col_n = col + dx[n];
                zin_n = input[(row_n, col_n)];
                zout_n = output[(row_n, col_n)];
                if zout_n == background_val {
                    if zin_n == nodata {
                        output[(row_n, col_n)] = nodata;
                        queue.push_back((row_n, col_n));
                    } else {
                        output[(row_n, col_n)] = zin_n;
                        // Push it onto the priority queue for the priority flood operation
                        minheap.push(GridCell {
                            row: row_n,
                            column: col_n,
                            priority: zin_n,
                        });
                    }
                    num_solved_cells += 1;
                }
            }

            if verbose {
                progress = (100.0_f64 * num_solved_cells as f64 / (num_cells - 1) as f64) as usize;
                if progress != old_progress {
                    println!("progress: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        // Perform the priority flood operation.
        while !minheap.is_empty() {
            let cell = minheap.pop().unwrap();
            row = cell.row;
            col = cell.column;
            zout = output[(row, col)];
            for n in 0..8 {
                row_n = row + dy[n];
                col_n = col + dx[n];
                zout_n = output[(row_n, col_n)];
                if zout_n == background_val {
                    zin_n = input[(row_n, col_n)];
                    if zin_n != nodata {
                        if zin_n < zout {
                            zin_n = zout;
                        } // We're in a depression. Raise the elevation.
                        output[(row_n, col_n)] = zin_n;
                        minheap.push(GridCell {
                            row: row_n,
                            column: col_n,
                            priority: zin_n,
                        });
                    } else {
                        // Interior nodata cells are still treated as nodata and are not filled.
                        output[(row_n, col_n)] = nodata;
                        num_solved_cells += 1;
                    }
                }
            }

            if verbose {
                num_solved_cells += 1;
                progress = (100.0_f64 * num_solved_cells as f64 / (num_cells - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Progress: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        // Reclassify the output such that all cells that are higher than the input are identified.
        let mut fid = 0f64;
        background_val = nodata;
        if zero_background {
            background_val = 0f64;
        }
        let mut visited: Array2D<i8> = Array2D::new(rows, columns, 0, -1)?;
        for row in 0..rows {
            for col in 0..columns {
                if output[(row, col)] > input[(row, col)] && visited[(row, col)] != 1 {
                    fid += 1f64;
                    output[(row, col)] = fid;
                    visited[(row, col)] = 1;
                    queue.push_back((row, col));
                    while !queue.is_empty() {
                        let cell = queue.pop_front().unwrap();
                        for n in 0..8 {
                            row_n = cell.0 + dy[n];
                            col_n = cell.1 + dx[n];
                            zout_n = output[(row_n, col_n)];
                            zin_n = input[(row_n, col_n)];
                            if zout_n > zin_n && visited[(row_n, col_n)] != 1 {
                                output[(row_n, col_n)] = fid;
                                visited[(row_n, col_n)] = 1;
                                queue.push_back((row_n, col_n));
                            }
                        }
                    }
                } else if output[(row, col)] == input[(row, col)] {
                    visited[(row, col)] = 1;
                    if input[(row, col)] != nodata {
                        output[(row, col)] = background_val;
                    } else {
                        output[(row, col)] = nodata;
                    }
                }
            }
            if verbose {
                progress = (100.0_f64 * row as f64 / (rows - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Clumping: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        let elapsed_time = get_formatted_elapsed_time(start);
        output.configs.data_type = DataType::F32;
        output.configs.palette = "qual.plt".to_string();
        output.configs.photometric_interp = PhotometricInterpretation::Categorical;
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

        Ok(())
    }
}

#[derive(PartialEq, Debug)]
struct GridCell {
    row: isize,
    column: isize,
    priority: f64,
}

impl Eq for GridCell {}

impl PartialOrd for GridCell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        other.priority.partial_cmp(&self.priority)
    }
}

impl Ord for GridCell {
    fn cmp(&self, other: &GridCell) -> Ordering {
        let ord = self.partial_cmp(other).unwrap();
        match ord {
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater,
            Ordering::Equal => ord,
        }
    }
}
