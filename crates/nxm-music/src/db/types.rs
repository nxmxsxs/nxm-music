use anyhow::Context as _;

#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(type_name = "CHAR")]
pub enum FileNodeType {
    #[sqlx(rename = "F")]
    File,
    #[sqlx(rename = "D")]
    Directory,
}

#[derive(Debug)]
#[repr(transparent)]
pub struct Blake3Hash(blake3::Hash);

impl From<blake3::Hash> for Blake3Hash {
    fn from(value: blake3::Hash) -> Self {
        Self(value)
    }
}

impl sqlx::Decode<'_, sqlx::Sqlite> for Blake3Hash {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'_>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let hash = <&[u8] as sqlx::Decode<'_, sqlx::Sqlite>>::decode(value)?;
        Ok(Blake3Hash(blake3::Hash::from_bytes(
            hash.try_into().context("Invalid BLAKE3 hash")?,
        )))
    }
}
