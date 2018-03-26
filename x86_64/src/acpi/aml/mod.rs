/*
 * Copyright (C) 2018, Pebble Developers.
 * See LICENCE.md
 */

mod opcodes;

use core::str;
use core::iter::Peekable;
use alloc::{String,Vec};
use memory::paging::VirtualAddress;
use super::AcpiInfo;
use bit_field::BitField;

#[derive(Clone)]
pub(super) enum TermObj
{
    NameSpaceModifierObj
    {
    },

    NamedObj
    {
    },
    
    Type1Opcode
    {
    },

    Type2Opcode
    {
    },
}

#[derive(Clone,Debug)]
pub(super) struct TermArg
{
}

struct AmlStream
{
    pub(self) address         : VirtualAddress,
    pub(self) remaining_bytes : usize,
}

impl AmlStream
{
    fn new(address : VirtualAddress, length : usize) -> AmlStream
    {
        AmlStream
        {
            address,
            remaining_bytes : length,
        }
    }
}

impl Iterator for AmlStream
{
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item>
    {
        match self.remaining_bytes
        {
            0 => None,

            _ =>
            {
                let byte = unsafe { *(self.address.ptr()) };
                self.address = self.address.offset(1);
                self.remaining_bytes -= 1;

                trace!("AML parser consumes: {:#x}", byte);

                Some(byte)
            },
        }
    }
}

pub(super) struct AmlParser
{
    pub(self) stream                : Peekable<AmlStream>,

    /*
     * This is set when we parse an object with a PkgLength. When it hits 0, we know we've parsed
     * the whole object, removing ambiguities.
     */
    pub(self) remaining_pkg_bytes   : usize,
}

impl AmlParser
{
    /// Create a new AmlParser, which will parse from the given address for the given number of
    /// bytes. Unsafe because the parser assumes the address is valid.
    pub(super) unsafe fn new(start : VirtualAddress, length : usize) -> AmlParser
    {
        AmlParser
        {
            stream              : AmlStream::new(start, length).peekable(),
            remaining_pkg_bytes : 0,
        }
    }

    fn consume<F>(&mut self, predicate : F) -> u8
        where F : Fn(u8) -> bool
    {
        let byte = self.stream.next().expect("Consume hit end of stream");
        
        if !predicate(byte)
        {
            panic!("AML parser consumed unexpected byte: {:#x}", byte);
        }

        byte
    }

    pub(super) fn parse(&mut self, acpi_info : &mut AcpiInfo)
    {
        while let Some(_) = self.stream.peek()
        {
            self.parse_term_obj(acpi_info);
        }
    }

    /*
     * This keeps parsing TermObjs until the end of the current PkgLength
     */
    fn parse_term_list(&mut self, acpi_info : &mut AcpiInfo) -> Vec<TermObj>
    {
        /*
         * TermList := Nothing | <TermObj TermList>
         */
        assert!(self.remaining_pkg_bytes > 0);

        let mut list = Vec::new();
        while self.remaining_pkg_bytes > 0
        {
            list.push(self.parse_term_obj(acpi_info));
        }
        list
    }

    fn parse_term_obj(&mut self, acpi_info : &mut AcpiInfo) -> TermObj
    {
        /*
         * TermObj := NameSpaceModifierObj | NamedObj | Type1Opcode | Type2Opcode
         */
        match self.stream.next().unwrap()
        {
            opcodes::SCOPE_OP =>
            {
                self.parse_scope_op(acpi_info)
            },

            opcodes::EXT_OP_PREFIX =>
            {
                match self.stream.next().unwrap()
                {
                    opcodes::OP_REGION_OP =>
                    {
                        /*
                         * DefOpRegion  := OpRegionOp NameString RegionSpace RegionOffset RegionLen
                         * RegionSpace  := ByteData
                         * RegionOffset := TermArg => Integer
                         * RegionLen    := TermArg => Integer
                         */
                        info!("Parsing OpReginOp");
                        let name_string     = self.parse_name_string();
                        info!("Name string is {:?}", name_string);
                        let region_space    = self.stream.next().unwrap();
                        info!("Region space is {:#x}", region_space);
                        let region_offset   = self.parse_term_arg();
                        info!("Region offset is {:?}", region_offset);
                        let region_len      = self.parse_term_arg();
                        info!("Region len is {:?}", region_len);
                        TermObj::NameSpaceModifierObj { } // TODO
                    },

                    _ => unimplemented!(),
                }
            },

            byte =>
            {
                panic!("Unrecognised AML opcode at top-level: {:#x}", byte);
            },
        }
    }

    fn parse_term_arg(&mut self) -> TermArg
    {
        /*
         * TermArg := Type2Opcode | DataObject | ArgObj | LocalObj
         */
        // TODO
        TermArg { }
    }

    fn parse_scope_op(&mut self, acpi_info : &mut AcpiInfo) -> TermObj
    {
        /*
         * DefScope := 0x10 PkgLength NameString TermList
         */
        let pkg_length = self.parse_pkg_length();
        info!("Pkg length = {},{}", pkg_length, self.remaining_pkg_bytes);
        let name_string = self.parse_name_string();
        info!("Name string: {}", name_string);
        let term_list = self.parse_term_list(acpi_info);

        // TODO: no
        term_list[0].clone()
    }

    fn parse_pkg_length(&mut self) -> u32
    {
        /*
         * PkgLength := PkgLeadByte |
         *              <PkgLeadByte ByteData> |
         *              <PkgLeadByte ByteData ByteData> |
         *              <PkgLeadByte ByteData ByteData ByteData> |
         *
         * The maximum value of this is 2^28, so we return u32
         */
        let lead_byte = self.stream.next().unwrap();
        let byte_data_count = lead_byte.get_bits(6..8);
        info!("PkgLength has {} data bytes", byte_data_count);

        if byte_data_count == 0
        {
            return lead_byte.get_bits(0..6) as u32;
        }

        let mut length = lead_byte.get_bits(0..4) as u32;

        for i in 0..byte_data_count
        {
            length += (self.stream.next().unwrap() as u32) << 4 + i * 8;
        }

        /*
         * Set the number of bytes left in the current structure, minus the size of this PkgLength.
         */
        self.remaining_pkg_bytes = length as usize - 1 - byte_data_count as usize;

        length
    }

    fn parse_name_seg(&mut self) -> [u8; 4]
    {
        [self.consume(is_lead_name_char),
         self.consume(is_name_char),
         self.consume(is_name_char),
         self.consume(is_name_char)]
    }

    fn parse_name_path(&mut self) -> String
    {
        /*
         * NamePath         := NameSeg | DualNamePath | MultiNamePath | NullPath
         * DualNamePath     := DualNamePrefix NameSeg NameSeg
         * MultiNamePath    := MultiNamePrefix SegCount{ByteData} NameSeg(..SegCount)
         * NameSeg          := <LeadNameChar NameChar NameChar NameChar>
         */
        let first_byte = *self.stream.peek().unwrap();

        match first_byte
        {
            opcodes::NULL_NAME =>
            {
                self.stream.next().unwrap();
                String::from("")
            },

            opcodes::DUAL_NAME_PREFIX =>
            {
                /*
                 * NamePath := DualNamePath
                 */
                self.stream.next().unwrap();
                let first = self.parse_name_seg();
                let second = self.parse_name_seg();

                let mut path = String::new();
                path.push_str(str::from_utf8(&first).unwrap());
                path.push_str(str::from_utf8(&second).unwrap());
                path
            },

            opcodes::MULTI_NAME_PREFIX =>
            {
                /*
                 * NamePath := MultiNamePath
                 */
                self.stream.next().unwrap();
                let seg_count = self.stream.next().unwrap();
                let mut path = String::new();

                for i in 0..seg_count
                {
                    path.push_str(str::from_utf8(&self.parse_name_seg()).unwrap());
                }

                path
            },

            _ =>
            {
                /*
                 * We've already parsed one of the bytes, so we manually parse the other three,
                 * rather than using `parse_name_seg`
                 */
                String::from(str::from_utf8(&[self.consume(is_name_char),
                                              self.consume(is_name_char),
                                              self.consume(is_name_char)]).unwrap())
            },
        }
    }

    fn parse_name_string(&mut self) -> String
    {

        /*
         * NameString   := <RootChar NamePath> | <PrefixPath NamePath>
         * PrefixPath   := Nothing | <'^' PrefixPath>
         */
        let first_byte = *self.stream.peek().unwrap();

        match first_byte
        {
            b'\\' =>
            {
                /*
                 * NameString := RootChar NamePath
                 */
                self.stream.next().unwrap();
                let mut name = String::from("\\");
                name += &self.parse_name_path();
                name
            },

            b'^' =>
            {
                /*
                 * NameString := PrefixPath NamePath
                 */
                self.stream.next().unwrap();
                let string = String::from("^");
                error!("Haven't actually parsed this name string, TODO");
                //TODO
                string
            },

            _ =>
            {
                /*
                 * NameString := PrefixPath[Nothing] NamePath
                 */
                let mut name = String::from(str::from_utf8(&[self.stream.next().unwrap()]).unwrap());
                name += &self.parse_name_path();
                name
            },
        }
    }
}

fn is_lead_name_char(byte : u8) -> bool
{
    (byte >= b'A' && byte <= b'Z') || byte == b'_'
}

fn is_digit_char(byte : u8) -> bool
{
    byte >= b'0' && byte <= b'9'
}

fn is_name_char(byte : u8) -> bool
{
    is_lead_name_char(byte) || is_digit_char(byte)
}
