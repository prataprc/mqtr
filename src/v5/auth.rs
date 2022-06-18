use crate::util::advance;
use crate::v5::{FixedHeader, PacketType, Property, PropertyType};
use crate::{Blob, Packetize, UserProperty, VarU32};
use crate::{Error, ErrorKind, ReasonCode, Result};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AuthReasonCode {
    Success = 0x00,
    ContinueAuthentication = 0x18,
    ReAuthenticate = 0x19,
}

impl TryFrom<u8> for AuthReasonCode {
    type Error = Error;

    fn try_from(val: u8) -> Result<AuthReasonCode> {
        match val {
            0x00 => Ok(AuthReasonCode::Success),
            0x18 => Ok(AuthReasonCode::ContinueAuthentication),
            0x19 => Ok(AuthReasonCode::ReAuthenticate),
            val => err!(ProtocolError, code: ProtocolError, "reason-code {:?}", val),
        }
    }
}

impl From<AuthReasonCode> for u8 {
    fn from(val: AuthReasonCode) -> u8 {
        match val {
            AuthReasonCode::Success => 0x00,
            AuthReasonCode::ContinueAuthentication => 0x18,
            AuthReasonCode::ReAuthenticate => 0x19,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Auth {
    code: Option<AuthReasonCode>,
    properties: Option<AuthProperties>,
}

impl Packetize for Auth {
    fn decode<T: AsRef<[u8]>>(stream: T) -> Result<(Self, usize)> {
        let stream: &[u8] = stream.as_ref();

        let (fh, n) = dec_field!(FixedHeader, stream, 0);
        fh.validate()?;

        let (code, n) = dec_field!(u8, stream, n);
        let code = Some(AuthReasonCode::try_from(code)?);

        let (properties, n) = dec_props!(AuthProperties, stream, n);

        let val = Auth { code, properties };

        Ok((val, n))
    }

    fn encode(&self) -> Result<Blob> {
        use crate::v5::insert_fixed_header;

        let mut data = Vec::with_capacity(64);

        let code = self.code.unwrap_or(AuthReasonCode::Success);
        data.extend_from_slice(u8::from(code).encode()?.as_ref());
        if let Some(properties) = &self.properties {
            data.extend_from_slice(properties.encode()?.as_ref());
        } else {
            data.extend_from_slice(VarU32(0).encode()?.as_ref());
        }

        let fh = FixedHeader::new(PacketType::Auth, VarU32(data.len().try_into()?))?;
        data = insert_fixed_header(fh, data)?;

        Ok(Blob::Large { data })
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct AuthProperties {
    authentication_method: String,
    authentication_data: Vec<u8>,
    reason_string: Option<String>,
    user_properties: Vec<UserProperty>,
}

impl Packetize for AuthProperties {
    fn decode<T: AsRef<[u8]>>(stream: T) -> Result<(Self, usize)> {
        use crate::v5::Property::*;

        let stream: &[u8] = stream.as_ref();

        let mut dups = [false; 256];
        let mut props = AuthProperties::default();

        let (len, mut n) = dec_field!(VarU32, stream, 0);
        let limit = usize::try_from(*len)? + n;

        let mut authentication_method: Option<String> = None;
        let mut authentication_data: Option<Vec<u8>> = None;
        while n < limit {
            let (property, m) = dec_field!(Property, stream, n);
            n = m;

            let pt = property.to_property_type();
            if pt != PropertyType::UserProp && dups[pt as usize] {
                err!(ProtocolError, code: ProtocolError, "duplicate property {:?}", pt)?
            }
            dups[pt as usize] = true;

            match property {
                AuthenticationMethod(val) => authentication_method = Some(val),
                AuthenticationData(val) => authentication_data = Some(val),
                ReasonString(val) => props.reason_string = Some(val),
                UserProp(val) => props.user_properties.push(val),
                _ => err!(
                    ProtocolError,
                    code: ProtocolError,
                    "{:?} found in disconnect properties",
                    pt
                )?,
            };
        }

        match authentication_method {
            Some(val) => props.authentication_method = val,
            None => err!(ProtocolError, code: ProtocolError, "missing auth-method")?,
        }
        match authentication_data {
            Some(val) => props.authentication_data = val,
            None => err!(ProtocolError, code: ProtocolError, "missing auth-data")?,
        }

        Ok((props, n))
    }

    fn encode(&self) -> Result<Blob> {
        use crate::v5::insert_property_len;

        let mut data = Vec::with_capacity(64);

        enc_prop!(data, AuthenticationMethod, self.authentication_method);
        enc_prop!(data, AuthenticationData, &self.authentication_data);
        enc_prop!(opt: data, ReasonString, &self.reason_string);

        for uprop in self.user_properties.iter() {
            enc_prop!(data, UserProp, uprop)
        }

        let data = insert_property_len(data.len(), data)?;

        Ok(Blob::Large { data })
    }
}