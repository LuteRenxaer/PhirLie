with open(r'F:\phira\prpr\src\ext.rs', 'r', encoding='utf-8') as f:
    content = f.read()


format_idx = content.find('pub fn format_number')

end_roman_idx = content.rfind('}\n', 0, format_idx) + 2


chinese_func = '''

pub fn to_chinese_numeral(num: u32) -> String {
    if num == 0 {
        return "零".to_string();
    }
    let digits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    let units = ["", "十", "百", "千"];
    let big_units = ["", "万", "亿"];
    
    let mut result = String::new();
    let mut n = num;
    let mut section = 0;
    
    while n > 0 {
        let mut section_num = n % 10000;
        n /= 10000;
        
        if section_num > 0 {
            let mut section_str = String::new();
            let mut zero_flag = false;
            let mut has_non_zero = false;
            
            for i in 0..4 {
                let digit = (section_num % 10) as usize;
                section_num /= 10;
                
                if digit == 0 {
                    if has_non_zero {
                        zero_flag = true;
                    }
                } else {
                    if zero_flag {
                        section_str.insert_str(0, "零");
                        zero_flag = false;
                    }
                    section_str.insert_str(0, units[i]);
                    section_str.insert_str(0, digits[digit]);
                    has_non_zero = true;
                }
            }
            
            section_str.push_str(big_units[section]);
            result.insert_str(0, &section_str);
        } else if section > 0 && !result.is_empty() && !result.starts_with("零") {
            result.insert_str(0, "零");
        }
        
        section += 1;
    }
    
    if result.starts_with("一十") {
        result = result.trim_start_matches('一').to_string();
    }
    
    result
}
'''


new_content = content[:end_roman_idx] + chinese_func + content[end_roman_idx:]


old_format = '''pub fn format_number(num: u32, roman_numerals: bool) -> String {
    if roman_numerals {
        to_roman_numeral(num)
    } else {
        num.to_string()
    }
}'''

new_format = '''pub fn format_number(num: u32, roman_numerals: bool, chinese_numerals: bool) -> String {
    if roman_numerals {
        to_roman_numeral(num)
    } else if chinese_numerals {
        to_chinese_numeral(num)
    } else {
        num.to_string()
    }
}'''

new_content = new_content.replace(old_format, new_format)

with open(r'F:\phira\prpr\src\ext.rs', 'w', encoding='utf-8') as f:
    f.write(new_content)

print('Done')
print('New format_number found:', 'pub fn format_number(num: u32, roman_numerals: bool, chinese_numerals: bool)' in new_content)
print('to_chinese_numeral found:', 'pub fn to_chinese_numeral' in new_content)
