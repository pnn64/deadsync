//! Lua 5.1's comparison and partition order over an already validated array.
use crate::Value;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) enum Failure {
    Order,
    Partitions,
}

pub(crate) fn scalar(
    values: &mut [Value],
    after: Value,
    before: Value,
    mut less: impl FnMut(Value, Value) -> bool,
) -> Result<(), Failure> {
    let mut ranges = [(0, 0); 32];
    ranges[0] = (1, values.len());
    let mut count = 1;
    while count != 0 {
        count -= 1;
        let (low, high) = ranges[count];
        if low >= high {
            continue;
        }
        if less(values[high - 1], values[low - 1]) {
            values.swap(low - 1, high - 1);
        }
        if high - low == 1 {
            continue;
        }
        let mid = (low + high) / 2;
        if less(values[mid - 1], values[low - 1]) {
            values.swap(mid - 1, low - 1);
        } else if less(values[high - 1], values[mid - 1]) {
            values.swap(mid - 1, high - 1);
        }
        if high - low == 2 {
            continue;
        }
        values.swap(mid - 1, high - 2);
        let pivot = values[high - 2];
        let mut i = low;
        let mut j = high - 1;
        loop {
            loop {
                i += 1;
                if !less(values.get(i - 1).copied().unwrap_or(after), pivot) {
                    break;
                }
                if i > high {
                    return Err(Failure::Order);
                }
            }
            loop {
                j -= 1;
                let value = if j == 0 { before } else { values[j - 1] };
                if !less(pivot, value) {
                    break;
                }
                if j < low {
                    return Err(Failure::Order);
                }
            }
            if j < i {
                break;
            }
            values.swap(i - 1, j - 1);
        }
        values.swap(high - 2, i - 1);
        if count + 2 > ranges.len() {
            return Err(Failure::Partitions);
        }
        let left = (low, i - 1);
        let right = (i + 1, high);
        if i - low < high - i {
            ranges[count..count + 2].copy_from_slice(&[right, left]);
        } else {
            ranges[count..count + 2].copy_from_slice(&[left, right]);
        }
        count += 2;
    }
    Ok(())
}
