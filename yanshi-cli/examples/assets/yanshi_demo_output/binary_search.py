二分查找（Binary Search）是一种在有序数组中查找特定元素的高效算法。它的时间复杂度为 O(log n)，其中 n 是数组的长度。下面是一个用 Python 实现的二分查找算法，包含详细注释。

```python
def binary_search(arr, target):
    """
    二分查找算法

    :param arr: 有序数组（必须是升序排列）
    :param target: 要查找的目标元素
    :return: 目标元素的索引（如果找到），否则返回 -1
    """
    # 定义左边界和右边界
    left = 0
    right = len(arr) - 1

    # 当左边界小于等于右边界时，继续查找
    while left <= right:
        # 计算中间位置
        mid = left + (right - left) // 2

        # 比较中间元素与目标元素
        if arr[mid] == target:
            # 如果相等，返回中间元素的索引
            return mid
        elif arr[mid] < target:
            # 如果目标元素大于中间元素，则在右半部分继续查找
            left = mid + 1
        else:
            # 如果目标元素小于中间元素，则在左半部分继续查找
            right = mid - 1

    # 如果循环结束仍未找到目标元素，返回 -1
    return -1

# 示例用法
if __name__ == "__main__":
    # 定义一个有序数组
    sorted_array = [2, 3, 4, 10, 40]
    # 定义要查找的目标元素
    target_element = 10

    # 调用二分查找函数
    result = binary_search(sorted_array, target_element)

    if result != -1:
        print(f"元素 {target_element} 在数组中的索引是: {result}")
    else:
        print(f"元素 {target_element} 不在数组中")
```

### 代码说明：

1. **函数定义**：
   - `binary_search(arr, target)`：接受一个有序数组 `arr` 和一个目标元素 `target` 作为参数。
   
2. **初始化边界**：
   - `left = 0`：左边界初始化为数组的第一个元素的索引。
   - `right = len(arr) - 1`：右边界初始化为数组的最后一个元素的索引。

3. **循环查找**：
   - `while left <= right`：当左边界小于等于右边界时，继续查找。
   - `mid = left + (right - left) // 2`：计算中间位置，使用整数除法避免溢出。
   
4. **比较中间元素**：
   - `if arr[mid] == target`：如果中间元素等于目标元素，返回中间元素的索引。
   - `elif arr[mid] < target`：如果目标元素大于中间元素，则在右半部分继续查找，更新左边界为 `mid + 1`。
   - `else`：如果目标元素小于中间元素，则在左半部分继续查找，更新右边界为 `mid - 1`。

5. **返回结果**：
   - 如果循环结束仍未找到目标元素，返回 `-1`。

6. **示例用法**：
   - 定义一个有序数组 `sorted_array` 和目标元素 `target_element`。
   - 调用 `binary_search` 函数并打印结果。

### 示例输出：
```
元素 10 在数组中的索引是: 3
```

这个实现确保了在有序数组中高效地查找目标元素，适用于需要快速查找元素的场景。