fn main() {
 println!("channels\tblock_size\tpasses\tencoder_storage\tsetup_bytes\tfirst_book_storage\tsetup_storage\tdecoder_storage");
 for (channels,block_size) in [(1,64),(2,1024),(8,8192),(255,512)] {
  for residue_passes in 1..=4 {
   let encoder=codec::PacketEncoder::new(codec::EncodeOptions{channels,block_size,residue_passes,..Default::default()}).unwrap();
   let setup=codec::Setup::parse(encoder.setup_packet(),encoder.identification(),&Default::default()).unwrap();
   let first=setup.codebooks()[1].storage_bytes(); let storage=setup.storage_bytes();
   let decoder=codec::PacketDecoder::new(setup).unwrap();
   println!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",channels,block_size,residue_passes,encoder.storage_bytes(),encoder.setup_packet().len(),first,storage,decoder.storage_bytes());
  }
 }
}
