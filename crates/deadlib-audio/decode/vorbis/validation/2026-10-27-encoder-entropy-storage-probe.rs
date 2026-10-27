fn main() {
    println!("channels\tblock_size\tpasses\tstorage_bytes\tsetup_bytes");
    for (channels, block_size) in [(1,64),(2,1024),(8,8192),(255,512)] {
        for residue_passes in 1..=4 {
            let encoder=codec::PacketEncoder::new(codec::EncodeOptions { channels,block_size,residue_passes,..Default::default() }).unwrap();
            println!("{}\t{}\t{}\t{}\t{}",channels,block_size,residue_passes,encoder.storage_bytes(),encoder.setup_packet().len());
        }
    }
}
