use super::*;
use crate::utils::ByteOrderReader;
use std::f64;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, BufWriter, Cursor, Error, ErrorKind, SeekFrom};
use std::mem;
use std::path::Path;

pub fn read_saga(
    file_name: &String,
    configs: &mut RasterConfigs,
    data: &mut Vec<f64>,
) -> Result<(), Error> {
    // read the header file
    // let header_file = file_name.replace(".sdat", ".sgrd");
    let header_file = Path::new(&file_name).with_extension("sgrd").into_os_string().into_string().unwrap();
    let f = File::open(header_file)?;
    let f = BufReader::new(f);
    let mut data_file_offset = 0u64;
    let mut top_to_bottom = false;
    let mut z_factor = 1.0;
    for line in f.lines() {
        let line_unwrapped = line.unwrap();
        //let line_split = line_unwrapped.split("\t");
        let line_split = line_unwrapped.split("=");
        let vec = line_split.collect::<Vec<&str>>();
        if vec[0].to_lowercase().contains("name") {
            configs.title = vec[1].replace("=", "").trim().to_string();
        } else if vec[0].to_lowercase().contains("description") {
            if vec[1].replace("=", "").trim() != "" {
                configs
                    .metadata
                    .push(vec[1].trim().replace("=", "").to_string());
            }
        } else if vec[0].to_lowercase().contains("unit") {
            if vec[1].replace("=", "").trim() != "" {
                configs.xy_units = vec[1].trim().replace("=", "").to_string();
            }
        } else if vec[0].to_lowercase().contains("datafile_offset") {
            data_file_offset = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<u64>()
                .unwrap();
        } else if vec[0].to_lowercase().contains("dataformat") {
            let data_format = vec[1].replace("=", "").trim().to_lowercase().to_string();
            match &data_format[..] {
                "bit" => {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "Reading of this kind of SAGA raster file is not currently supported",
                    ))
                }
                "byte_unsigned" => configs.data_type = DataType::U8,
                "byte" => configs.data_type = DataType::U8,
                "shortint_unsigned" => configs.data_type = DataType::U16,
                "shortint" => configs.data_type = DataType::I16,
                "integer_unsigned" => configs.data_type = DataType::U32,
                "integer" => configs.data_type = DataType::I32,
                "float" => configs.data_type = DataType::F32,
                "double" => configs.data_type = DataType::F64,
                _ => {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "Reading of this kind of SAGA raster file is not currently supported",
                    ))
                }
            }
        } else if vec[0].to_lowercase().contains("byteorder_big") {
            if vec[1].replace("=", "").trim().to_lowercase().contains("f")
                || vec[1]
                    .replace("=", "")
                    .trim()
                    .to_lowercase()
                    .contains("lsb")
            {
                configs.endian = Endianness::LittleEndian;
            } else {
                configs.endian = Endianness::BigEndian;
            }
        } else if vec[0].to_lowercase().contains("position_xmin") {
            configs.west = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<f64>()
                .unwrap();
        } else if vec[0].to_lowercase().contains("position_ymin") {
            configs.south = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<f64>()
                .unwrap();
        } else if vec[0].to_lowercase().contains("cellcount_x") {
            configs.columns = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<usize>()
                .unwrap();
        } else if vec[0].to_lowercase().contains("cellcount_y") {
            configs.rows = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<usize>()
                .unwrap();
        } else if vec[0].to_lowercase().contains("cellsize") {
            configs.resolution_x = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<f64>()
                .unwrap();
            configs.resolution_y = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<f64>()
                .unwrap();
        } else if vec[0].to_lowercase().contains("z_factor") {
            z_factor = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<f64>()
                .unwrap();
        } else if vec[0].to_lowercase().contains("nodata_value") {
            configs.nodata = vec[1]
                .replace("=", "")
                .trim()
                .to_string()
                .parse::<f64>()
                .unwrap();
        } else if vec[0].to_lowercase().contains("toptobottom") {
            top_to_bottom = vec[1].replace("=", "").trim().to_lowercase().contains("t")
        }
    }

    configs.north = configs.south + configs.resolution_y * configs.rows as f64;
    configs.east = configs.west + configs.resolution_x * configs.columns as f64;

    if z_factor < 0.0 && (configs.data_type == DataType::F32 || configs.data_type == DataType::F64)
    {
        configs.data_type = DataType::F32;
    }

    let mut row_start = 0;
    if !top_to_bottom {
        row_start = configs.rows - 1;
    }

    data.reserve(configs.rows * configs.columns);
    
    // read the data file
    // let data_file = file_name.replace(".sgrd", ".sdat");
    let data_file = Path::new(&file_name).with_extension("sdat").into_os_string().into_string().unwrap();
    let mut f = File::open(data_file.clone())?;
    f.seek(SeekFrom::Start(data_file_offset))?;

    let data_size = if configs.data_type == DataType::F64 {
        8
    } else if configs.data_type == DataType::F32
        || configs.data_type == DataType::I32
        || configs.data_type == DataType::U32
    {
        4
    } else if configs.data_type == DataType::I16 || configs.data_type == DataType::U16 {
        2
    } else {
        // DataType::U8 or I8
        1
    };

    let num_cells = configs.rows * configs.columns;
    data.clear();
    data.reserve(num_cells);
    for _ in 0..num_cells {
        data.push(configs.nodata);
    }

    let buf_size = 1_000_000usize;
    let mut j = 0;
    let mut row = row_start;
    let mut col = 0;
    let mut k: usize;
    while j < num_cells {
        let mut buffer = vec![0; buf_size * data_size];

        f.read(&mut buffer)?;

        let mut bor = if configs.endian == Endianness::LittleEndian {
            ByteOrderReader::<Cursor<Vec<u8>>>::new(Cursor::new(buffer), Endianness::LittleEndian)
        } else {
            ByteOrderReader::<Cursor<Vec<u8>>>::new(Cursor::new(buffer), Endianness::BigEndian)
        };
        bor.seek(0);

        match configs.data_type {
            DataType::F64 => {
                for _ in 0..buf_size {
                    k = row * configs.columns + col;
                    data[k] = bor.read_f64()? as f64 * z_factor;

                    j += 1;
                    if j == num_cells {
                        break;
                    }
                    col += 1;
                    if col >= configs.columns {
                        col = 0;
                        if !top_to_bottom {
                            row -= 1;
                        } else {
                            row += 1;
                        }
                    }
                }
            }
            DataType::F32 => {
                for _ in 0..buf_size {
                    k = row * configs.columns + col;
                    data[k] = bor.read_f32()? as f64 * z_factor;

                    j += 1;
                    if j == num_cells {
                        break;
                    }
                    col += 1;
                    if col >= configs.columns {
                        col = 0;
                        if !top_to_bottom {
                            row -= 1;
                        } else {
                            row += 1;
                        }
                    }
                }
            }
            DataType::I32 => {
                for _ in 0..buf_size {
                    k = row * configs.columns + col;
                    data[k] = bor.read_i32()? as f64 * z_factor;

                    j += 1;
                    if j == num_cells {
                        break;
                    }
                    col += 1;
                    if col >= configs.columns {
                        col = 0;
                        if !top_to_bottom {
                            row -= 1;
                        } else {
                            row += 1;
                        }
                    }
                }
            }
            DataType::U32 => {
                for _ in 0..buf_size {
                    k = row * configs.columns + col;
                    data[k] = bor.read_u32()? as f64 * z_factor;

                    j += 1;
                    if j == num_cells {
                        break;
                    }
                    col += 1;
                    if col >= configs.columns {
                        col = 0;
                        if !top_to_bottom {
                            row -= 1;
                        } else {
                            row += 1;
                        }
                    }
                }
            }
            DataType::I16 => {
                for _ in 0..buf_size {
                    k = row * configs.columns + col;
                    data[k] = bor.read_i16()? as f64 * z_factor;

                    j += 1;
                    if j == num_cells {
                        break;
                    }
                    col += 1;
                    if col >= configs.columns {
                        col = 0;
                        if !top_to_bottom {
                            row -= 1;
                        } else {
                            row += 1;
                        }
                    }
                }
            }
            DataType::U16 => {
                for _ in 0..buf_size {
                    k = row * configs.columns + col;
                    data[k] = bor.read_u16()? as f64 * z_factor;

                    j += 1;
                    if j == num_cells {
                        break;
                    }
                    col += 1;
                    if col >= configs.columns {
                        col = 0;
                        if !top_to_bottom {
                            row -= 1;
                        } else {
                            row += 1;
                        }
                    }
                }
            }
            DataType::I8 => {
                for _ in 0..buf_size {
                    k = row * configs.columns + col;
                    data[k] = bor.read_i8()? as f64 * z_factor;
                    j += 1;
                    if j == num_cells {
                        break;
                    }
                    col += 1;
                    if col >= configs.columns {
                        col = 0;
                        if !top_to_bottom {
                            row -= 1;
                        } else {
                            row += 1;
                        }
                    }
                }
            }
            DataType::U8 => {
                for _ in 0..buf_size {
                    k = row * configs.columns + col;
                    data[k] = bor.read_u8()? as f64 * z_factor;
                    j += 1;
                    if j == num_cells {
                        break;
                    }
                    col += 1;
                    if col >= configs.columns {
                        col = 0;
                        if !top_to_bottom {
                            row -= 1;
                        } else {
                            row += 1;
                        }
                    }
                }
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    "Raster data type is unknown.",
                ));
            }
        }
    }

    Ok(())
}

pub fn write_saga<'a>(r: &'a mut Raster) -> Result<(), Error> {
    // figure out the minimum and maximum values
    for val in &r.data {
        let v = *val;
        if v != r.configs.nodata {
            if v < r.configs.minimum {
                r.configs.minimum = v;
            }
            if v > r.configs.maximum {
                r.configs.maximum = v;
            }
        }
    }

    if r.configs.display_min == f64::INFINITY {
        r.configs.display_min = r.configs.minimum;
    }
    if r.configs.display_max == f64::NEG_INFINITY {
        r.configs.display_max = r.configs.maximum;
    }

    // Save the header file
    // let header_file = r.file_name.replace(".sdat", ".sgrd");
    let header_file = Path::new(&r.file_name).with_extension("sgrd").into_os_string().into_string().unwrap();
    let f = File::create(header_file.clone())?;
    let mut writer = BufWriter::new(f);

    // get the short file NAME
    let short_name: String = match Path::new(&header_file).file_name().unwrap().to_str() {
        Some(n) => n.to_string().to_lowercase(),
        None => "".to_string(),
    };

    writer.write_all(format!("NAME\t= {}\n", short_name).as_bytes())?;

    if r.configs.metadata.len() > 0 {
        writer.write_all(format!("DESCRIPTION\t= {}\n", r.configs.metadata[0]).as_bytes())?;
    } else {
        writer.write_all("DESCRIPTION\t=\n".as_bytes())?;
    }

    if r.configs.xy_units != "not specified" {
        writer.write_all(format!("UNIT\t= {}\n", r.configs.xy_units).as_bytes())?;
    } else {
        writer.write_all("UNIT\t=\n".as_bytes())?;
    }

    writer.write_all("DATAFILE_OFFSET\t= 0\n".as_bytes())?;

    match r.configs.data_type {
        DataType::F64 => {
            writer.write_all("DATAFORMAT\t= DOUBLE\n".as_bytes())?;
        }
        DataType::F32 => {
            writer.write_all("DATAFORMAT\t= FLOAT\n".as_bytes())?;
        }
        DataType::I32 => {
            writer.write_all("DATAFORMAT\t= INTEGER\n".as_bytes())?;
        }
        DataType::U32 => {
            writer.write_all("DATAFORMAT\t= INTEGER_UNSIGNED\n".as_bytes())?;
        }
        DataType::I16 => {
            writer.write_all("DATAFORMAT\t= SHORTINT\n".as_bytes())?;
        }
        DataType::U16 => {
            writer.write_all("DATAFORMAT\t= SHORTINT_UNSIGNED\n".as_bytes())?;
        }
        DataType::U8 => {
            writer.write_all("DATAFORMAT\t= BYTE_UNSIGNED\n".as_bytes())?;
        }
        DataType::I8 => {
            writer.write_all("DATAFORMAT\t= BYTE\n".as_bytes())?;
        }
        _ => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!(
                    "Raster data type {:?} not supported in this format.",
                    r.configs.data_type
                ),
            ));
        }
    }

    if r.configs.endian == Endianness::LittleEndian {
        writer.write_all("BYTEORDER_BIG\t= FALSE\n".as_bytes())?;
    } else {
        writer.write_all("BYTEORDER_BIG\t= TRUE\n".as_bytes())?;
    }

    writer.write_all(format!("POSITION_XMIN\t= {}\n", r.configs.west).as_bytes())?;

    writer.write_all(format!("POSITION_YMIN\t= {}\n", r.configs.south).as_bytes())?;

    writer.write_all(format!("CELLCOUNT_X\t= {}\n", r.configs.columns).as_bytes())?;

    writer.write_all(format!("CELLCOUNT_Y\t= {}\n", r.configs.rows).as_bytes())?;

    writer.write_all(
        format!(
            "CELLSIZE\t= {}\n",
            (r.configs.resolution_x + r.configs.resolution_y) / 2.0
        )
        .as_bytes(),
    )?;

    writer.write_all("Z_FACTOR\t= 1.000000\n".as_bytes())?;

    writer.write_all(format!("NODATA_VALUE\t= {}\n", r.configs.nodata).as_bytes())?;

    writer.write_all("TOPTOBOTTOM\t= FALSE\n".as_bytes())?;

    let _ = writer.flush();

    // write the data file
    // let data_file = r.file_name.replace(".sgrd", ".sdat");
    let data_file = Path::new(&r.file_name).with_extension("sdat").into_os_string().into_string().unwrap();
    let f = File::create(&data_file)?;
    let mut writer = BufWriter::new(f);

    let mut u16_bytes: [u8; 2];
    let mut u32_bytes: [u8; 4];
    let mut u64_bytes: [u8; 8];
    let mut i: usize;
    match r.configs.data_type {
        DataType::F64 => {
            for row in (0..r.configs.rows).rev() {
                for col in 0..r.configs.columns {
                    i = row * r.configs.columns + col;
                    u64_bytes = unsafe { mem::transmute(r.data[i]) };
                    writer.write(&u64_bytes)?;
                }
            }
        }
        DataType::F32 => {
            for row in (0..r.configs.rows).rev() {
                for col in 0..r.configs.columns {
                    i = row * r.configs.columns + col;
                    u32_bytes = unsafe { mem::transmute(r.data[i] as f32) };
                    writer.write(&u32_bytes)?;
                }
            }
        }
        DataType::I32 => {
            for row in (0..r.configs.rows).rev() {
                for col in 0..r.configs.columns {
                    i = row * r.configs.columns + col;
                    u32_bytes = unsafe { mem::transmute(r.data[i] as i32) };
                    writer.write(&u32_bytes)?;
                }
            }
        }
        DataType::U32 => {
            for row in (0..r.configs.rows).rev() {
                for col in 0..r.configs.columns {
                    i = row * r.configs.columns + col;
                    u32_bytes = unsafe { mem::transmute(r.data[i] as u32) };
                    writer.write(&u32_bytes)?;
                }
            }
        }
        DataType::I16 => {
            for row in (0..r.configs.rows).rev() {
                for col in 0..r.configs.columns {
                    i = row * r.configs.columns + col;
                    u16_bytes = unsafe { mem::transmute(r.data[i] as i16) };
                    writer.write(&u16_bytes)?;
                }
            }
        }
        DataType::U16 => {
            for row in (0..r.configs.rows).rev() {
                for col in 0..r.configs.columns {
                    i = row * r.configs.columns + col;
                    u16_bytes = unsafe { mem::transmute(r.data[i] as u16) };
                    writer.write(&u16_bytes)?;
                }
            }
        }
        DataType::U8 | DataType::I8 => {
            for row in (0..r.configs.rows).rev() {
                for col in 0..r.configs.columns {
                    i = row * r.configs.columns + col;
                    writer.write(&[r.data[i] as u8])?;
                }
            }
        }
        _ => {
            return Err(Error::new(
                ErrorKind::NotFound,
                "Raster data type is unsupported.",
            ));
        }
    }

    let _ = writer.flush();

    Ok(())
}
