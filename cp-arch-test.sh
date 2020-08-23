set -e


#rm -rf a b c d; mkdir a b c d; ln -s ../c a; ln -s ../c b;
#cp -RL a b d
#./target/debug/coreutils cp cp -RL a b d
# cp: error: Option 'dereference' not yet implemented.
#rm -rf a b c d

rm -rf dir dir-copy-archive-rust
mkdir dir
echo "a" > dir/1
echo "b" > dir/2
touch -d "2 hours ago" dir/1
chmod 705 dir/*
cd dir
ln -s 1 1.link
ln -s 2 2.link
cd -
cp --archive dir dir-copy-archive
ls -al dir-copy-archive|grep "rwx---r-x"
ls -al dir-copy-archive|grep "1.link -> 1"
./target/debug/coreutils cp --archive -v dir dir-copy-archive-rust
ls -al dir-copy-archive-rust|grep "rwx---r-x"
# les symlinks ne sont pas créés
ls -al dir-copy-archive-rust|grep "1.link -> 1"
